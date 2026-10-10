//! Model-domain composition: the provider registry, the offline switch, the
//! router, and the vault-backed credential resolver.

use super::ServicesError;
use aidex_core::EntityId;
use aidex_drive::DrivePaths;
use aidex_events::{EventKind, EventScope, EventSink};
use aidex_providers::{
    CredentialError, CredentialResolver, ModelEvent, ModelId, ModelObserver, ModelRef, OfflineMode,
    ProviderId, ProviderRegistry, SecretBytes,
};
use aidex_router::{NetworkState, Router as ModelRouter};
use aidex_vault::{ConsumerScope, SecretName, VaultError, VaultErrorKind, VaultService};
use std::{fmt, sync::Arc, time::Duration};

/// Lifetime of the single-use handle issued for one provider request.
const CREDENTIAL_HANDLE_TTL: Duration = Duration::from_secs(30);

/// Resolves provider credentials through vault handles scoped to the
/// consumer `provider:<id>`: a short-lived handle is issued, resolved, and
/// revoked for every request, so a secret is released only to the provider
/// its policy names. Nothing is cached and nothing is read from the
/// environment or configuration.
pub struct VaultCredentials {
    vault: Arc<VaultService>,
}

impl VaultCredentials {
    pub fn new(vault: Arc<VaultService>) -> Self {
        Self { vault }
    }
}

impl fmt::Debug for VaultCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultCredentials").finish_non_exhaustive()
    }
}

fn credential_error(error: &VaultError, secret_name: &str) -> CredentialError {
    let name = || secret_name.to_owned();
    match error.kind() {
        VaultErrorKind::Locked => CredentialError::Locked,
        VaultErrorKind::NotFound => CredentialError::Unavailable {
            reason: "no vault exists on this drive; create one with `aidex vault init`".into(),
        },
        VaultErrorKind::SecretNotFound => CredentialError::NotFound {
            secret_name: name(),
        },
        VaultErrorKind::ConsumerNotAllowed | VaultErrorKind::ForbiddenConsumer => {
            CredentialError::Denied {
                secret_name: name(),
            }
        }
        kind => CredentialError::Unavailable {
            reason: format!("vault error ({})", kind.as_str()),
        },
    }
}

impl CredentialResolver for VaultCredentials {
    fn resolve(
        &self,
        provider: &ProviderId,
        secret_name: &str,
    ) -> Result<SecretBytes, CredentialError> {
        let name = SecretName::new(secret_name).map_err(|_| CredentialError::NotFound {
            secret_name: secret_name.to_owned(),
        })?;
        let consumer = ConsumerScope::provider(provider.as_str())
            .map_err(|error| credential_error(&error, secret_name))?;
        let handle = self
            .vault
            .issue_handle(&name, &consumer, CREDENTIAL_HANDLE_TTL)
            .map_err(|error| credential_error(&error, secret_name))?;
        let resolved = self.vault.resolve(&handle, &consumer);
        self.vault.revoke_handle(&handle.handle_id);
        let secret = resolved.map_err(|error| credential_error(&error, secret_name))?;
        Ok(SecretBytes::new(secret.expose().to_vec()))
    }
}

/// Longest `reason` text recorded in a model event.
const MAX_EVENT_REASON: usize = 1024;

/// Records model-domain notifications as typed events v2 (global scope).
///
/// `model.selected` and `model.fallback` map to their catalog kinds and an
/// offline-switch change to `policy.changed`; provider registration and
/// default-model changes have no catalog entry yet and are not recorded.
/// Event identifiers are `EntityId`s, so characters an upstream model name may
/// contain but an id may not (`/`, `@`, `+`) are written as `.`; a fallback
/// endpoint is `<provider>:<model>`. Emission is best effort: a refused or
/// failed event never fails the model operation that produced it.
pub struct EventsModelObserver {
    sink: Arc<dyn EventSink>,
}

impl EventsModelObserver {
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Self { sink }
    }
}

impl fmt::Debug for EventsModelObserver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventsModelObserver")
            .finish_non_exhaustive()
    }
}

/// An event identifier for a model name (see [`EventsModelObserver`]).
fn model_entity(model: &ModelId) -> Option<EntityId> {
    let text: String = model
        .as_str()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':') {
                ch
            } else {
                '.'
            }
        })
        .collect();
    EntityId::new(text).ok()
}

fn reference_entity(model: &ModelRef) -> Option<EntityId> {
    let model_id = model_entity(&model.model_id)?;
    EntityId::new(format!("{}:{}", model.provider_id, model_id)).ok()
}

fn bounded(text: String) -> String {
    if text.len() <= MAX_EVENT_REASON {
        return text;
    }
    let mut end = MAX_EVENT_REASON;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// The catalog event for a model notification, if the catalog has one.
pub fn model_event_kind(event: &ModelEvent) -> Option<EventKind> {
    match event {
        ModelEvent::Selected { model, reasons, .. } => Some(EventKind::ModelSelected {
            provider_id: model.provider_id.as_entity_id().clone(),
            model_id: model_entity(&model.model_id)?,
            reason: bounded(reasons.join("; ")),
        }),
        ModelEvent::Fallback(fallback) => Some(EventKind::ModelFallback {
            from: reference_entity(&fallback.from)?,
            to: reference_entity(&fallback.to)?,
            reason: bounded(format!("{}: {}", fallback.reason, fallback.message)),
        }),
        ModelEvent::NetworkModeChanged { offline } => Some(EventKind::PolicyChanged {
            profile: None,
            summary: if *offline {
                "offline mode on: remote model providers are refused".into()
            } else {
                "offline mode off: remote model providers are allowed".into()
            },
        }),
        ModelEvent::DefaultChanged { .. }
        | ModelEvent::ProviderAdded { .. }
        | ModelEvent::ProviderRemoved { .. } => None,
    }
}

impl ModelObserver for EventsModelObserver {
    fn observe(&self, event: &ModelEvent) {
        if let Some(kind) = model_event_kind(event) {
            // Best effort by design: notifications never fail the model call.
            let _ = self.sink.emit(EventScope::global(), kind);
        }
    }
}

/// The daemon's model services.
pub struct ModelServices {
    registry: Arc<ProviderRegistry>,
    offline: Arc<OfflineMode>,
    router: ModelRouter,
    observer: Arc<dyn ModelObserver>,
}

impl ModelServices {
    /// Load the offline switch and configured providers from the drive.
    pub fn open(
        paths: &DrivePaths,
        vault: Arc<VaultService>,
        observer: Arc<dyn ModelObserver>,
    ) -> Result<Self, ServicesError> {
        let offline = Arc::new(
            OfflineMode::load(paths)
                .map_err(|error| ServicesError::new("network", error.code(), error.to_string()))?,
        );
        let registry = ProviderRegistry::load(
            paths,
            Arc::new(VaultCredentials::new(vault)),
            Arc::clone(&offline) as _,
        )
        .map_err(|error| ServicesError::new("providers", error.code(), error.to_string()))?
        .with_observer(Arc::clone(&observer));
        Ok(Self {
            registry: Arc::new(registry),
            offline,
            router: ModelRouter::new(),
            observer,
        })
    }

    /// Configured and runtime model providers.
    pub fn registry(&self) -> &Arc<ProviderRegistry> {
        &self.registry
    }

    /// The persisted offline switch every remote provider call is gated on.
    pub fn offline(&self) -> &Arc<OfflineMode> {
        &self.offline
    }

    pub fn router(&self) -> &ModelRouter {
        &self.router
    }

    pub fn observer(&self) -> &Arc<dyn ModelObserver> {
        &self.observer
    }

    pub fn network(&self) -> NetworkState {
        if self.offline.is_offline() {
            NetworkState::Offline
        } else {
            NetworkState::Online
        }
    }
}

impl fmt::Debug for ModelServices {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModelServices")
            .field("registry", &self.registry)
            .field("offline", &self.offline.is_offline())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aidex_events::MemoryEventSink;
    use aidex_providers::{FallbackEvent, ProviderErrorKind};

    #[test]
    fn model_notifications_become_typed_catalog_events() {
        let sink = Arc::new(MemoryEventSink::new());
        let observer = EventsModelObserver::new(sink.clone());
        observer.observe(&ModelEvent::Selected {
            model: ModelRef::parse("vllm/meta-llama/Llama-3.1-8B").unwrap(),
            fallbacks: Vec::new(),
            reasons: vec!["task code".into(), "selected".into()],
        });
        observer.observe(&ModelEvent::Fallback(FallbackEvent {
            from: ModelRef::parse("local/qwen").unwrap(),
            to: ModelRef::parse("openai/gpt-4o@latest").unwrap(),
            reason: ProviderErrorKind::Unavailable,
            message: "provider unavailable: connection refused".into(),
        }));
        observer.observe(&ModelEvent::NetworkModeChanged { offline: true });
        observer.observe(&ModelEvent::DefaultChanged { model: None });
        let events: Vec<EventKind> = sink.events().into_iter().map(|e| e.event).collect();
        assert_eq!(events.len(), 3);
        let EventKind::ModelSelected {
            provider_id,
            model_id,
            reason,
        } = &events[0]
        else {
            panic!("{:?}", events[0]);
        };
        assert_eq!(
            (provider_id.as_str(), model_id.as_str(), reason.as_str()),
            ("vllm", "meta-llama.Llama-3.1-8B", "task code; selected")
        );
        let EventKind::ModelFallback { from, to, reason } = &events[1] else {
            panic!("{:?}", events[1]);
        };
        assert_eq!(
            (from.as_str(), to.as_str()),
            ("local:qwen", "openai:gpt-4o.latest")
        );
        assert!(reason.starts_with("unavailable: "));
        assert_eq!(events[2].name(), "policy.changed");
    }
}
