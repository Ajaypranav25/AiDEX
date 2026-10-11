//! Model providers for AiDEX: provider-neutral chat contracts, the synchronous
//! [`ModelProvider`] trait, a deterministic [`FakeProvider`], an
//! OpenAI-compatible HTTP provider, request-time vault credentials, the
//! offline egress gate, and the [`ProviderRegistry`].
//!
//! Provider failures (unavailable, unauthorized, rate limited, context
//! overflow, offline refusal, ...) are explicit [`ProviderError`] outcomes.
//! Credentials are resolved per request through a [`CredentialResolver`] and
//! never stored in configuration, events, errors, `Debug` output, or model
//! context.

mod chat;
mod cloud;
mod config;
mod credentials;
mod egress;
mod error;
mod fake;
mod ids;
mod model;
mod observer;
mod openai;
mod presets;
mod provider;
mod registry;

pub use aidex_core::{CancelReason, CancelToken};
pub use chat::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, MAX_MESSAGES, MAX_STOP_SEQUENCES,
    MAX_TOOLS, ResponseFormat, Role, StreamEvent, ToolCall, ToolChoice, ToolSpec, Usage,
    is_tool_name,
};
pub use config::{
    AuthConfig, CONFIG_VERSION, ConfigError, ModelConfig, NETWORK_FILE, NetworkConfig,
    PROVIDERS_FILE, ProviderConfig, ProviderKind, ProvidersConfig, config_file,
};
pub use credentials::{
    CredentialResolver, NoVault, SecretBytes, StaticCredentials, is_secret_name,
};
pub use egress::{EgressGate, OfflineMode};
pub use error::{CredentialError, ProviderError, ProviderErrorKind};
pub use fake::{FakeOutcome, FakeProvider, FakeScript, FakeTurn};
pub use ids::{InvalidId, ModelId, ModelRef, ProviderId};
pub use model::{
    ASSUMED_CONTEXT_WINDOW, CapabilitySource, CostHint, Locality, ModelCapabilities,
    ModelDescriptor, ModelRole, ToolCallSupport,
};
pub use observer::{FallbackEvent, ModelEvent, ModelObserver, NoopObserver, RecordingObserver};
pub use openai::{
    BaseUrl, DiscoveredModel, HttpOptions, InvalidProvider, InvalidUrl, OpenAiClient,
    OpenAiCompatibleProvider, Scheme,
};
pub use presets::{ProviderPreset, provider_presets};
pub use provider::{HealthStatus, ModelProvider, ProviderDescriptor, ProviderHealth};
pub use registry::{
    ModelListing, ProviderEntry, ProviderFailure, ProviderOrigin, ProviderRegistry, RegistryError,
};
