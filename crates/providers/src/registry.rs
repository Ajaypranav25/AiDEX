//! The provider registry: configured and runtime providers, aggregated model
//! listings with per-provider failures, and the persisted default model.

use std::{
    collections::BTreeMap,
    fmt,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
    thread,
};

use aidex_core::ServiceErrorCode;
use aidex_drive::DrivePaths;
use serde::{Deserialize, Serialize};

use crate::{
    ConfigError, CredentialResolver, EgressGate, HttpOptions, ModelDescriptor, ModelEvent,
    ModelObserver, ModelProvider, ModelRef, NoopObserver, OpenAiCompatibleProvider, ProviderConfig,
    ProviderDescriptor, ProviderError, ProviderErrorKind, ProviderHealth, ProviderId,
    ProvidersConfig,
    config::{PROVIDERS_FILE, prepare_config_file, write_toml},
};

/// How a provider entered the registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOrigin {
    /// Declared in `providers.toml`.
    Configured,
    /// Registered by a running service (for example local inference); not persisted.
    Runtime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderEntry {
    pub descriptor: ProviderDescriptor,
    pub origin: ProviderOrigin,
}

/// A provider whose model listing failed; reported alongside the models of
/// the providers that succeeded, never hidden.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderFailure {
    pub provider_id: ProviderId,
    pub kind: ProviderErrorKind,
    pub message: String,
}

impl ProviderFailure {
    pub fn new(provider_id: ProviderId, error: &ProviderError) -> Self {
        Self {
            provider_id,
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelListing {
    /// Models ordered by `provider/model`.
    pub models: Vec<ModelDescriptor>,
    pub failures: Vec<ProviderFailure>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RegistryError {
    Config(ConfigError),
    /// A provider configuration was rejected.
    Invalid(String),
    Duplicate(ProviderId),
    NotFound(ProviderId),
    /// The provider does not list this model.
    ModelNotListed(ModelRef),
    /// The provider was registered at runtime and is not in `providers.toml`.
    NotConfigured(ProviderId),
    Provider {
        provider_id: ProviderId,
        error: ProviderError,
    },
}

impl RegistryError {
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Config(error) => error.code(),
            Self::Invalid(_) => ServiceErrorCode::InvalidArgument,
            Self::Duplicate(_) | Self::NotConfigured(_) => ServiceErrorCode::Conflict,
            Self::NotFound(_) | Self::ModelNotListed(_) => ServiceErrorCode::NotFound,
            Self::Provider { error, .. } => error.code(),
        }
    }
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(f, "provider configuration: {error}"),
            Self::Invalid(message) => f.write_str(message),
            Self::Duplicate(id) => write!(f, "provider `{id}` already exists"),
            Self::NotFound(id) => write!(f, "provider `{id}` is not registered"),
            Self::ModelNotListed(model) => {
                write!(f, "model `{model}` is not offered by its provider")
            }
            Self::NotConfigured(id) => write!(
                f,
                "provider `{id}` is managed by a running service and is not in providers.toml"
            ),
            Self::Provider { provider_id, error } => write!(f, "provider `{provider_id}`: {error}"),
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<ConfigError> for RegistryError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

struct Entry {
    provider: Arc<dyn ModelProvider>,
    origin: ProviderOrigin,
}

struct Inner {
    config: ProvidersConfig,
    providers: BTreeMap<ProviderId, Entry>,
}

/// All model providers known to this process.
///
/// Configured providers come from `private_config/providers.toml`; runtime
/// providers (such as the local llama.cpp service) are registered by their
/// owning service and are never written to disk. Methods take `&self`, so one
/// `Arc<ProviderRegistry>` is shared by every caller.
pub struct ProviderRegistry {
    config_path: Option<PathBuf>,
    resolver: Arc<dyn CredentialResolver>,
    gate: Arc<dyn EgressGate>,
    observer: Arc<dyn ModelObserver>,
    options: HttpOptions,
    inner: RwLock<Inner>,
}

impl fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let providers: Vec<String> = self
            .read()
            .providers
            .keys()
            .map(ToString::to_string)
            .collect();
        f.debug_struct("ProviderRegistry")
            .field("config_path", &self.config_path)
            .field("providers", &providers)
            .finish_non_exhaustive()
    }
}

impl ProviderRegistry {
    /// Load configured providers from the drive. A missing file is an empty
    /// registry; an invalid file or provider entry is an error.
    pub fn load(
        paths: &DrivePaths,
        resolver: Arc<dyn CredentialResolver>,
        gate: Arc<dyn EgressGate>,
    ) -> Result<Self, RegistryError> {
        Self::load_with(paths, resolver, gate, HttpOptions::default())
    }

    pub fn load_with(
        paths: &DrivePaths,
        resolver: Arc<dyn CredentialResolver>,
        gate: Arc<dyn EgressGate>,
        options: HttpOptions,
    ) -> Result<Self, RegistryError> {
        let path = prepare_config_file(paths, PROVIDERS_FILE)?;
        let config = ProvidersConfig::load(&path)?;
        let registry = Self {
            config_path: Some(path),
            resolver,
            gate,
            observer: Arc::new(NoopObserver),
            options,
            inner: RwLock::new(Inner {
                config: ProvidersConfig::default(),
                providers: BTreeMap::new(),
            }),
        };
        let mut providers = BTreeMap::new();
        for provider in &config.providers {
            providers.insert(
                provider.id.clone(),
                Entry {
                    provider: Arc::new(registry.build(provider)?),
                    origin: ProviderOrigin::Configured,
                },
            );
        }
        *registry.write() = Inner { config, providers };
        Ok(registry)
    }

    /// A registry without persistence (tests and embedding).
    pub fn in_memory(resolver: Arc<dyn CredentialResolver>, gate: Arc<dyn EgressGate>) -> Self {
        Self {
            config_path: None,
            resolver,
            gate,
            observer: Arc::new(NoopObserver),
            options: HttpOptions::default(),
            inner: RwLock::new(Inner {
                config: ProvidersConfig::default(),
                providers: BTreeMap::new(),
            }),
        }
    }

    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn ModelObserver>) -> Self {
        self.observer = observer;
        self
    }

    fn read(&self) -> RwLockReadGuard<'_, Inner> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write(&self) -> RwLockWriteGuard<'_, Inner> {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn build(&self, config: &ProviderConfig) -> Result<OpenAiCompatibleProvider, RegistryError> {
        OpenAiCompatibleProvider::with_options(
            config,
            Arc::clone(&self.resolver),
            Arc::clone(&self.gate),
            self.options,
        )
        .map_err(|error| RegistryError::Invalid(error.0))
    }

    fn persist(&self, config: &ProvidersConfig) -> Result<(), RegistryError> {
        if let Some(path) = &self.config_path {
            write_toml(path, config)?;
        }
        Ok(())
    }

    /// Validate, persist, and activate a configured provider.
    pub fn add_provider(
        &self,
        config: ProviderConfig,
    ) -> Result<ProviderDescriptor, RegistryError> {
        let provider = self.build(&config)?;
        let descriptor = provider.descriptor().clone();
        let mut inner = self.write();
        if inner.providers.contains_key(&config.id) {
            return Err(RegistryError::Duplicate(config.id));
        }
        let mut next = inner.config.clone();
        next.providers.push(config);
        self.persist(&next)?;
        inner.config = next;
        inner.providers.insert(
            descriptor.id.clone(),
            Entry {
                provider: Arc::new(provider),
                origin: ProviderOrigin::Configured,
            },
        );
        drop(inner);
        self.observer.observe(&ModelEvent::ProviderAdded {
            provider_id: descriptor.id.clone(),
            provider_kind: descriptor.kind,
            locality: descriptor.locality,
        });
        Ok(descriptor)
    }

    /// Remove a configured provider (and the default model if it pointed there).
    pub fn remove_provider(&self, id: &ProviderId) -> Result<(), RegistryError> {
        let mut inner = self.write();
        match inner.providers.get(id).map(|entry| entry.origin) {
            None => return Err(RegistryError::NotFound(id.clone())),
            Some(ProviderOrigin::Runtime) => return Err(RegistryError::NotConfigured(id.clone())),
            Some(ProviderOrigin::Configured) => {}
        }
        let mut next = inner.config.clone();
        next.providers.retain(|provider| &provider.id != id);
        let cleared = next
            .default_model()
            .is_some_and(|model| &model.provider_id == id);
        if cleared {
            next.default_model = None;
        }
        self.persist(&next)?;
        inner.config = next;
        inner.providers.remove(id);
        drop(inner);
        self.observer.observe(&ModelEvent::ProviderRemoved {
            provider_id: id.clone(),
        });
        if cleared {
            self.observer
                .observe(&ModelEvent::DefaultChanged { model: None });
        }
        Ok(())
    }

    /// Register a runtime provider (not persisted).
    pub fn register(&self, provider: Box<dyn ModelProvider>) -> Result<(), RegistryError> {
        self.register_shared(Arc::from(provider))
    }

    /// Register a runtime provider the caller keeps a handle to.
    pub fn register_shared(&self, provider: Arc<dyn ModelProvider>) -> Result<(), RegistryError> {
        let descriptor = provider.descriptor().clone();
        let mut inner = self.write();
        if inner.providers.contains_key(&descriptor.id) {
            return Err(RegistryError::Duplicate(descriptor.id));
        }
        inner.providers.insert(
            descriptor.id.clone(),
            Entry {
                provider,
                origin: ProviderOrigin::Runtime,
            },
        );
        drop(inner);
        self.observer.observe(&ModelEvent::ProviderAdded {
            provider_id: descriptor.id,
            provider_kind: descriptor.kind,
            locality: descriptor.locality,
        });
        Ok(())
    }

    /// Remove a runtime provider.
    pub fn unregister(&self, id: &ProviderId) -> Result<(), RegistryError> {
        let mut inner = self.write();
        match inner.providers.get(id).map(|entry| entry.origin) {
            None => return Err(RegistryError::NotFound(id.clone())),
            Some(ProviderOrigin::Configured) => {
                return Err(RegistryError::Invalid(format!(
                    "provider `{id}` is configured; remove it with `aidex provider remove`"
                )));
            }
            Some(ProviderOrigin::Runtime) => {}
        }
        inner.providers.remove(id);
        drop(inner);
        self.observer.observe(&ModelEvent::ProviderRemoved {
            provider_id: id.clone(),
        });
        Ok(())
    }

    /// Every provider, ordered by id.
    pub fn providers(&self) -> Vec<ProviderEntry> {
        self.read()
            .providers
            .values()
            .map(|entry| ProviderEntry {
                descriptor: entry.provider.descriptor().clone(),
                origin: entry.origin,
            })
            .collect()
    }

    pub fn provider(&self, id: &ProviderId) -> Option<Arc<dyn ModelProvider>> {
        self.read()
            .providers
            .get(id)
            .map(|entry| Arc::clone(&entry.provider))
    }

    /// The stored configuration of a configured provider.
    pub fn provider_config(&self, id: &ProviderId) -> Option<ProviderConfig> {
        self.read()
            .config
            .providers
            .iter()
            .find(|provider| &provider.id == id)
            .cloned()
    }

    /// Models from every provider, listed concurrently. A failing provider is
    /// reported in [`ModelListing::failures`]; the others are still listed.
    pub fn models(&self) -> ModelListing {
        let providers: Vec<(ProviderId, Arc<dyn ModelProvider>)> = self
            .read()
            .providers
            .iter()
            .map(|(id, entry)| (id.clone(), Arc::clone(&entry.provider)))
            .collect();
        let results: Vec<(ProviderId, Result<Vec<ModelDescriptor>, ProviderError>)> =
            thread::scope(|scope| {
                let handles: Vec<_> = providers
                    .iter()
                    .map(|(id, provider)| (id, scope.spawn(|| provider.models())))
                    .collect();
                handles
                    .into_iter()
                    .map(|(id, handle)| {
                        let result = handle.join().unwrap_or_else(|_| {
                            Err(ProviderError::unavailable("model listing panicked"))
                        });
                        (id.clone(), result)
                    })
                    .collect()
            });
        let mut listing = ModelListing::default();
        for (id, result) in results {
            match result {
                Ok(models) => listing.models.extend(
                    models
                        .into_iter()
                        .filter(|model| model.model.provider_id == id),
                ),
                Err(error) => listing.failures.push(ProviderFailure::new(id, &error)),
            }
        }
        listing.models.sort_by(|a, b| a.model.cmp(&b.model));
        listing
    }

    pub fn default_model(&self) -> Option<ModelRef> {
        self.read().config.default_model()
    }

    /// Persist `model` as the default after checking its provider lists it.
    pub fn set_default_model(&self, model: ModelRef) -> Result<ModelDescriptor, RegistryError> {
        let provider = self
            .provider(&model.provider_id)
            .ok_or_else(|| RegistryError::NotFound(model.provider_id.clone()))?;
        let descriptor = provider
            .models()
            .map_err(|error| RegistryError::Provider {
                provider_id: model.provider_id.clone(),
                error,
            })?
            .into_iter()
            .find(|descriptor| descriptor.model == model)
            .ok_or_else(|| RegistryError::ModelNotListed(model.clone()))?;
        self.store_default(Some(model))?;
        Ok(descriptor)
    }

    pub fn clear_default_model(&self) -> Result<(), RegistryError> {
        self.store_default(None)
    }

    fn store_default(&self, model: Option<ModelRef>) -> Result<(), RegistryError> {
        let mut inner = self.write();
        let mut next = inner.config.clone();
        next.default_model = model.as_ref().map(ToString::to_string);
        self.persist(&next)?;
        inner.config = next;
        drop(inner);
        self.observer.observe(&ModelEvent::DefaultChanged { model });
        Ok(())
    }

    pub fn health(&self, id: &ProviderId) -> Result<ProviderHealth, RegistryError> {
        self.provider(id)
            .map(|provider| provider.health())
            .ok_or_else(|| RegistryError::NotFound(id.clone()))
    }
}
