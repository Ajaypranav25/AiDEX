//! A scoped OpenAI-compatible adapter which becomes unusable when its process ends.
use aidex_core::CancelToken;
use aidex_providers::{
    ChatRequest, ChatResponse, ModelDescriptor, ModelProvider, OpenAiCompatibleProvider,
    ProviderDescriptor, ProviderError, ProviderHealth, ProviderKind, StreamEvent,
};
use aidex_sandbox::ManagedProcess;
use std::sync::{Arc, Weak};
pub(crate) struct LocalProvider {
    inner: OpenAiCompatibleProvider,
    descriptor: ProviderDescriptor,
    process: Weak<ManagedProcess>,
    model_id: String,
    model_name: String,
}
impl LocalProvider {
    pub fn new(
        inner: OpenAiCompatibleProvider,
        process: &Arc<ManagedProcess>,
        model_id: &str,
        model_name: &str,
    ) -> Self {
        let mut descriptor = inner.descriptor().clone();
        descriptor.kind = ProviderKind::LlamaCpp;
        Self {
            inner,
            descriptor,
            process: Arc::downgrade(process),
            model_id: model_id.into(),
            model_name: model_name.into(),
        }
    }
    fn live(&self) -> Result<(), ProviderError> {
        if self.process.upgrade().is_some_and(|p| p.is_running()) {
            Ok(())
        } else {
            Err(ProviderError::unavailable(
                "managed local inference is stopped",
            ))
        }
    }
}
impl ModelProvider for LocalProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }
    fn models(&self) -> Result<Vec<ModelDescriptor>, ProviderError> {
        self.live()?;
        self.inner.models().map(|mut models| {
            for model in &mut models {
                model.display_name = self.model_name.clone();
            }
            models
        })
    }
    fn chat(
        &self,
        request: &ChatRequest,
        on_event: &mut dyn FnMut(&StreamEvent),
        cancel: &CancelToken,
    ) -> Result<ChatResponse, ProviderError> {
        self.live()?;
        if request.model.as_str() != self.model_id {
            return Err(ProviderError::InvalidRequest(
                "request is outside the loaded model scope".into(),
            ));
        }
        self.inner.chat(request, on_event, cancel)
    }
    fn health(&self) -> ProviderHealth {
        match self.live() {
            Ok(()) => self.inner.health(),
            Err(error) => ProviderHealth::from_error(&error),
        }
    }
}
