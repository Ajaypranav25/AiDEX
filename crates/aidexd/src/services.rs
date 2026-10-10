//! Daemon-wide service composition.
//!
//! [`Services`] is built once at startup and shared with handlers as
//! `Arc<Services>`. Domain services (events, workspaces, sessions, vault, ...)
//! are added here as fields constructed in [`Services::open`]; their JSON-RPC
//! adapters live in `methods/<domain>.rs`.

mod models;

pub use models::{ModelServices, VaultCredentials};

use crate::{endpoint, lifecycle::ShutdownSignal, methods::events::event_error_code};
use aidex_core::ServiceErrorCode;
use aidex_drive::DrivePaths;
use aidex_events::EventLog;
use aidex_vault::{VaultService, VaultServiceConfig};
use aidex_workspace::WorkspaceRegistry;
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// Event database, relative to the drive's `.aidex/runtime` directory.
pub const EVENT_FILE: &str = "events.sqlite";
/// Events retained per workspace before the oldest expire.
pub const EVENT_RETENTION: u64 = 10_000;

pub struct Services {
    drive_root: PathBuf,
    paths: DrivePaths,
    pid: u32,
    started_at: Instant,
    shutdown: ShutdownSignal,
    events: EventLog,
    vault: Arc<VaultService>,
    workspaces: WorkspaceRegistry,
    models: ModelServices,
}

impl Services {
    /// Open every domain service rooted at `drive_root`.
    ///
    /// Add each domain service here: open it, map its error with
    /// `ServicesError::new("<domain>", error.code(), error.to_string())`, and
    /// store it in a field with an accessor.
    pub fn open(drive_root: &Path, shutdown: ShutdownSignal) -> Result<Self, ServicesError> {
        if !drive_root.is_dir() {
            return Err(ServicesError::new(
                "daemon",
                ServiceErrorCode::Unavailable,
                format!("selected drive is unavailable: {}", drive_root.display()),
            ));
        }
        // Services run only on an initialized schema-v2 AiDEX drive; their state lives in the
        // directories its manifest declares. Opening never creates or repairs the manifest.
        let paths = DrivePaths::open(drive_root)
            .map_err(|error| ServicesError::new("drive", error.code(), error.to_string()))?;
        let state_error = |error: endpoint::EndpointError| {
            ServicesError::new("daemon", error.code(), error.to_string())
        };
        let runtime = endpoint::ensure_state_dir(drive_root).map_err(state_error)?;
        let event_path = runtime.join(EVENT_FILE);
        endpoint::refuse_symlink(&event_path).map_err(state_error)?;
        let events = EventLog::open(&event_path, EVENT_RETENTION).map_err(|error| {
            ServicesError::new("events", event_error_code(&error), error.to_string())
        })?;
        let vault_dir = paths
            .ensure_dir(&paths.vault())
            .map_err(|error| ServicesError::new("vault", error.code(), error.to_string()))?;
        let vault = VaultService::open(vault_dir, VaultServiceConfig::default())
            .map_err(|error| ServicesError::new("vault", error.code(), error.to_string()))?;
        let workspaces = WorkspaceRegistry::open(&paths)
            .map_err(|error| ServicesError::new("workspace", error.code(), error.to_string()))?;
        let vault = Arc::new(vault);
        // Typed `model.*` events need the events v2 catalog; until it is wired the
        // observer discards notifications (they are still returned to callers).
        let models = ModelServices::open(
            &paths,
            Arc::clone(&vault),
            Arc::new(aidex_providers::NoopObserver),
        )?;
        Ok(Self {
            drive_root: drive_root.to_path_buf(),
            paths,
            pid: std::process::id(),
            started_at: Instant::now(),
            shutdown,
            events,
            vault,
            workspaces,
            models,
        })
    }

    pub fn drive_root(&self) -> &Path {
        &self.drive_root
    }

    /// Absolute service directories declared by the drive manifest (`aidex-drive`).
    /// Build service paths from these accessors and create them with `ensure_dir`.
    pub fn paths(&self) -> &DrivePaths {
        &self.paths
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Ask the daemon to stop; the listener notices within one poll interval.
    pub fn shutdown(&self) -> &ShutdownSignal {
        &self.shutdown
    }

    /// Durable, workspace-scoped event log (`aidex-events`).
    pub fn events(&self) -> &EventLog {
        &self.events
    }

    /// The drive's encrypted vault (`aidex-vault`). Handle issuance/resolution for the
    /// broker are Rust methods on [`VaultService`]; they are not RPC methods.
    pub fn vault(&self) -> &Arc<VaultService> {
        &self.vault
    }

    /// Workspace registry and confinement (`aidex-workspace`).
    pub fn workspaces(&self) -> &WorkspaceRegistry {
        &self.workspaces
    }

    /// Model providers, the router, and the offline switch
    /// (`aidex-providers`, `aidex-router`).
    pub fn models(&self) -> &ModelServices {
        &self.models
    }
}

/// Domain services may hold secrets or handles; never derive `Debug` here.
impl fmt::Debug for Services {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Services")
            .field("drive_root", &self.drive_root)
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

/// A domain service failed to open at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServicesError {
    service: &'static str,
    code: ServiceErrorCode,
    message: String,
}

impl ServicesError {
    pub fn new(service: &'static str, code: ServiceErrorCode, message: impl Into<String>) -> Self {
        Self {
            service,
            code,
            message: message.into(),
        }
    }

    pub fn service(&self) -> &'static str {
        self.service
    }

    pub fn code(&self) -> ServiceErrorCode {
        self.code
    }
}

impl fmt::Display for ServicesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} service failed to start: {}",
            self.service, self.message
        )
    }
}

impl std::error::Error for ServicesError {}

/// Unit-test fixtures for daemon modules.
#[cfg(test)]
pub(crate) mod testing {
    /// A temporary, initialized schema-v2 AiDEX drive (no removable-media check).
    pub(crate) fn temp_drive() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("temp dir");
        aidex_drive::init_with(
            root.path(),
            aidex_drive::InitOptions {
                require_removable: false,
            },
        )
        .expect("initialize test drive");
        root
    }
}
