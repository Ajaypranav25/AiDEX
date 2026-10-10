//! Model-domain composition: the provider registry, the offline switch, the
//! router, and the vault-backed credential resolver.

use super::ServicesError;
use aidex_drive::DrivePaths;
use aidex_providers::{
    CredentialError, CredentialResolver, ModelObserver, OfflineMode, ProviderId, ProviderRegistry,
    SecretBytes,
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
