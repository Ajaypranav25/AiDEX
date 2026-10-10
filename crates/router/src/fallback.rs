//! Running a routed chat with explicit, privacy-respecting fallback.

use std::sync::Arc;

use aidex_providers::{
    CancelReason, CancelToken, ChatRequest, ChatResponse, FallbackEvent, Locality, ModelProvider,
    ModelRef, ProviderError, ProviderId, ProviderRegistry, StreamEvent,
};

use crate::RouteDecision;

/// Finds the provider serving a model.
pub trait ProviderLookup {
    fn provider(&self, id: &ProviderId) -> Option<Arc<dyn ModelProvider>>;
}

impl ProviderLookup for ProviderRegistry {
    fn provider(&self, id: &ProviderId) -> Option<Arc<dyn ModelProvider>> {
        ProviderRegistry::provider(self, id)
    }
}

impl<F> ProviderLookup for F
where
    F: Fn(&ProviderId) -> Option<Arc<dyn ModelProvider>>,
{
    fn provider(&self, id: &ProviderId) -> Option<Arc<dyn ModelProvider>> {
        self(id)
    }
}

/// Whether an error lets the next candidate be tried: transient failures
/// (see [`ProviderError::is_retryable`]), an offline refusal (later local
/// candidates still work), or a model the provider no longer serves.
pub fn is_fallback_eligible(error: &ProviderError) -> bool {
    error.is_retryable()
        || matches!(
            error,
            ProviderError::OfflineRefused { .. } | ProviderError::ModelNotFound { .. }
        )
}

/// Run `request` on the decision's primary model, falling back in order.
///
/// The request's `model` is replaced by each candidate's model id. A
/// candidate is skipped (with a fallback event) when its provider is missing
/// or when it is remote and `decision.allow_remote` is false, so a decision
/// can never fall back from local to cloud when privacy or offline mode forbid
/// it. Only [`is_fallback_eligible`] errors move on; anything else, and any
/// cancellation or expired deadline, returns at once. `on_fallback` receives a
/// `model.fallback` event before each next attempt; streamed deltas of a failed
/// attempt should be discarded by the caller when it sees that event.
pub fn run_with_fallback(
    decision: &RouteDecision,
    lookup: &dyn ProviderLookup,
    request: &ChatRequest,
    on_event: &mut dyn FnMut(&StreamEvent),
    cancel: &CancelToken,
    on_fallback: &mut dyn FnMut(&FallbackEvent),
) -> Result<(ChatResponse, ModelRef), ProviderError> {
    let candidates: Vec<&ModelRef> = decision.candidates().collect();
    for (index, model) in candidates.iter().copied().enumerate() {
        if let Some(reason) = cancel.state() {
            return Err(match reason {
                CancelReason::Cancelled => ProviderError::Cancelled,
                CancelReason::DeadlineExceeded => ProviderError::Timeout,
            });
        }
        let (attempt, skipped) = match lookup.provider(&model.provider_id) {
            None => (
                Err(ProviderError::unavailable(format!(
                    "provider `{}` is not registered",
                    model.provider_id
                ))),
                true,
            ),
            Some(provider)
                if provider.descriptor().locality == Locality::Remote && !decision.allow_remote =>
            {
                (
                    Err(ProviderError::PolicyDenied {
                        reason: "remote models are not allowed for this request".into(),
                    }),
                    true,
                )
            }
            Some(provider) => {
                let mut attempt = request.clone();
                attempt.model = model.model_id.clone();
                (provider.chat(&attempt, on_event, cancel), false)
            }
        };
        let error = match attempt {
            Ok(response) => return Ok((response, model.clone())),
            Err(error) => error,
        };
        let stopped = cancel.state().is_some() || matches!(error, ProviderError::Cancelled);
        let next = candidates.get(index + 1).copied();
        match next {
            Some(next) if !stopped && (skipped || is_fallback_eligible(&error)) => {
                on_fallback(&FallbackEvent {
                    from: model.clone(),
                    to: next.clone(),
                    reason: error.kind(),
                    message: error.to_string(),
                });
            }
            _ => return Err(error),
        }
    }
    Err(ProviderError::unavailable("the route has no candidates"))
}
