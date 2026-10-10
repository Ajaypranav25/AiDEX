//! The egress gate every remote provider call passes before opening a socket.

use std::{
    fmt,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use aidex_drive::DrivePaths;

use crate::{
    ProviderError, ProviderId,
    config::{
        CONFIG_VERSION, ConfigError, NETWORK_FILE, NetworkConfig, prepare_config_file, write_toml,
    },
};

/// Decides whether a provider may contact a remote host.
///
/// Providers call [`EgressGate::check_remote`] before any network I/O for a
/// remote endpoint; a refusal means no connection is attempted. Loopback
/// endpoints are local and are not gated. The broker replaces [`OfflineMode`]
/// with a policy-backed gate later.
pub trait EgressGate: Send + Sync {
    fn check_remote(&self, provider: &ProviderId, host: &str) -> Result<(), ProviderError>;
}

/// The offline switch: while offline every remote provider call is refused
/// with [`ProviderError::OfflineRefused`].
///
/// [`OfflineMode::load`] persists the switch in `private_config/network.toml`.
/// Going offline takes effect in memory even if persisting fails (fail closed);
/// going online is persisted first and applied only if that succeeds.
pub struct OfflineMode {
    offline: AtomicBool,
    store: Option<PathBuf>,
    write: Mutex<()>,
}

impl OfflineMode {
    /// An in-memory switch (not persisted).
    pub fn new(offline: bool) -> Self {
        Self {
            offline: AtomicBool::new(offline),
            store: None,
            write: Mutex::new(()),
        }
    }

    /// Load the persisted switch from the drive (online when never set).
    pub fn load(paths: &DrivePaths) -> Result<Self, ConfigError> {
        let path = prepare_config_file(paths, NETWORK_FILE)?;
        let config = NetworkConfig::load(&path)?;
        Ok(Self {
            offline: AtomicBool::new(config.offline),
            store: Some(path),
            write: Mutex::new(()),
        })
    }

    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::SeqCst)
    }

    pub fn set_offline(&self, offline: bool) -> Result<(), ConfigError> {
        let _guard = self
            .write
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if offline {
            self.offline.store(true, Ordering::SeqCst);
        }
        if let Some(path) = &self.store {
            write_toml(
                path,
                &NetworkConfig {
                    version: CONFIG_VERSION,
                    offline,
                },
            )?;
        }
        self.offline.store(offline, Ordering::SeqCst);
        Ok(())
    }
}

impl fmt::Debug for OfflineMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OfflineMode")
            .field("offline", &self.is_offline())
            .field("persisted", &self.store.is_some())
            .finish()
    }
}

impl EgressGate for OfflineMode {
    fn check_remote(&self, _provider: &ProviderId, host: &str) -> Result<(), ProviderError> {
        if self.is_offline() {
            Err(ProviderError::OfflineRefused {
                host: host.to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_refuses_and_persists_across_loads() {
        let root = tempfile::tempdir().unwrap();
        aidex_drive::init_with(
            root.path(),
            aidex_drive::InitOptions {
                require_removable: false,
            },
        )
        .unwrap();
        let paths = DrivePaths::open(root.path()).unwrap();
        let provider = ProviderId::new("openai").unwrap();
        let mode = OfflineMode::load(&paths).unwrap();
        assert!(mode.check_remote(&provider, "api.openai.com").is_ok());
        mode.set_offline(true).unwrap();
        assert!(matches!(
            mode.check_remote(&provider, "api.openai.com"),
            Err(ProviderError::OfflineRefused { .. })
        ));
        assert!(OfflineMode::load(&paths).unwrap().is_offline());
        mode.set_offline(false).unwrap();
        assert!(!OfflineMode::load(&paths).unwrap().is_offline());
    }
}
