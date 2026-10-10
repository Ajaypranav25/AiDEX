//! [`LocalLlamaProvider`]: registered models as an `aidex_providers::ModelProvider`
//! (`local/<id>`, locality local). Chat loads the model lazily, delegates to the
//! OpenAI-compatible client pointed at the managed llama-server, and leaves the
//! server resident until the idle timeout.

use aidex_core::CancelToken;
use aidex_providers::{
    AuthConfig, CapabilitySource, ChatRequest, ChatResponse, Locality, ModelCapabilities,
    ModelDescriptor, ModelProvider, ModelRef, ProviderDescriptor, ProviderError, ProviderHealth,
    ProviderKind, StreamEvent,
};

use crate::{InferenceError, LocalInference, LocalModelId, RegistryEntry};

pub struct LocalLlamaProvider {
    descriptor: ProviderDescriptor,
    inference: LocalInference,
}

impl std::fmt::Debug for LocalLlamaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalLlamaProvider")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl LocalLlamaProvider {
    pub fn new(inference: LocalInference) -> Self {
        Self {
            descriptor: ProviderDescriptor {
                id: inference.provider_id().clone(),
                kind: ProviderKind::LlamaCpp,
                base_url: None,
                locality: Locality::Local,
                auth: AuthConfig::None,
            },
            inference,
        }
    }

    fn descriptor_for(&self, entry: &RegistryEntry) -> ModelDescriptor {
        let capabilities = ModelCapabilities {
            chat: entry.capabilities.chat,
            streaming: true,
            tool_calls: entry.capabilities.tool_calls,
            json_schema_output: entry.capabilities.json_schema,
            vision: false,
            embeddings: false,
        };
        ModelDescriptor::new(
            ModelRef::new(self.descriptor.id.clone(), entry.id.model_id().clone()),
            Locality::Local,
            self.inference.context_window(entry),
        )
        .with_display_name(entry.display_name.clone())
        .with_capabilities(capabilities)
        .with_roles(entry.roles.iter().copied())
        .with_min_ram_bytes(entry.memory.min_ram_bytes)
        .with_source(CapabilitySource::Configured)
    }
}

impl ModelProvider for LocalLlamaProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn models(&self) -> Result<Vec<ModelDescriptor>, ProviderError> {
        let listing = self
            .inference
            .models()
            .map_err(InferenceError::into_provider_error)?;
        Ok(listing
            .entries
            .iter()
            .filter(|entry| entry.capabilities.chat)
            .map(|entry| self.descriptor_for(entry))
            .collect())
    }

    fn chat(
        &self,
        request: &ChatRequest,
        on_event: &mut dyn FnMut(&StreamEvent),
        cancel: &CancelToken,
    ) -> Result<ChatResponse, ProviderError> {
        let id = LocalModelId::new(request.model.as_str()).map_err(|_| {
            ProviderError::ModelNotFound {
                model: request.model.to_string(),
            }
        })?;
        let result = request
            .validate()
            .map_err(InferenceError::Provider)
            .and_then(|()| self.inference.chat(&id, request, on_event, cancel));
        result.map_err(InferenceError::into_provider_error)
    }

    /// Ready when a runtime for this host is installed and at least one model is
    /// registered (servers start lazily, so no process is contacted).
    fn health(&self) -> ProviderHealth {
        let started = std::time::Instant::now();
        let check = || -> Result<(), ProviderError> {
            let models = self
                .inference
                .models()
                .map_err(InferenceError::into_provider_error)?;
            if models.entries.is_empty() {
                return Err(ProviderError::unavailable(
                    "no local model is registered; import one with `aidex model import <FILE>`",
                ));
            }
            let platform = aidex_drive::platform_runtime_name().ok_or_else(|| {
                ProviderError::unavailable("local inference is not supported on this platform")
            })?;
            let installed = self
                .inference
                .runtimes()
                .map_err(InferenceError::into_provider_error)?
                .installed;
            if !installed
                .runtimes
                .iter()
                .any(|runtime| runtime.manifest.platform == platform)
            {
                return Err(ProviderError::unavailable(format!(
                    "no llama.cpp runtime is installed for {platform}; run `aidex runtime install <ARCHIVE>`"
                )));
            }
            Ok(())
        };
        match check() {
            Ok(()) => ProviderHealth::ready(Some(
                u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            )),
            Err(error) => ProviderHealth::from_error(&error),
        }
    }
}
