//! Daemon-wide service composition.
//!
//! [`Services`] is built once at startup and shared with handlers as
//! `Arc<Services>`. Domain services (events, workspaces, sessions, vault, ...)
//! are added here as fields constructed in [`Services::open`]; their JSON-RPC
//! adapters live in `methods/<domain>.rs`.

mod models;

pub use models::{EventsModelObserver, ModelServices, VaultCredentials, model_event_kind};

use crate::{endpoint, lifecycle::ShutdownSignal};
use aidex_core::ServiceErrorCode;
use aidex_drive::DrivePaths;
use aidex_events::{EventLog, RetentionPolicy};
use aidex_inference::{InferenceOptions, LocalInference};
use aidex_providers::RegistryError;
use aidex_sandbox::Sandbox;
use aidex_vault::{VaultService, VaultServiceConfig};
use aidex_workspace::WorkspaceRegistry;
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// Event database inside the drive's session directory (`DrivePaths::sessions()`).
pub const EVENT_FILE: &str = "events.sqlite3";
/// PR #40's event database inside `.aidex/runtime/`. Superseded: imported once into
/// [`EVENT_FILE`] when that log is empty, then left untouched and never written.
pub const LEGACY_EVENT_FILE: &str = "events.sqlite";
/// Durable events retained before the oldest expire.
pub const EVENT_RETENTION_EVENTS: u64 = 100_000;
/// Durable events older than this expire (90 days).
pub const EVENT_RETENTION_AGE: Duration = Duration::from_secs(90 * 24 * 60 * 60);

/// The daemon's event retention policy.
pub fn event_retention() -> RetentionPolicy {
    RetentionPolicy {
        max_events: Some(EVENT_RETENTION_EVENTS),
        max_age: Some(EVENT_RETENTION_AGE),
    }
}

/// Where the daemon keeps the event log on a drive.
pub fn event_log_path(paths: &DrivePaths) -> PathBuf {
    paths.sessions().join(EVENT_FILE)
}

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
    inference: LocalInference,
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
        let events = open_events(&paths, &runtime.join(LEGACY_EVENT_FILE))?;
        let vault_dir = paths
            .ensure_dir(&paths.vault())
            .map_err(|error| ServicesError::new("vault", error.code(), error.to_string()))?;
        let vault = VaultService::open(vault_dir, VaultServiceConfig::default())
            .map_err(|error| ServicesError::new("vault", error.code(), error.to_string()))?;
        let workspaces = WorkspaceRegistry::open(&paths)
            .map_err(|error| ServicesError::new("workspace", error.code(), error.to_string()))?;
        let vault = Arc::new(vault);
        // Model notifications are recorded as typed `model.*` events.
        let models = ModelServices::open(
            &paths,
            Arc::clone(&vault),
            Arc::new(EventsModelObserver::new(Arc::new(events.clone()))),
        )?;
        let inference = open_inference(&paths, &models, &events)?;
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
            inference,
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

    /// The drive's durable event log (`aidex-events`); clone it to get an `EventSink`.
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

    /// Local GGUF inference (`aidex-inference`): the model registry, runtime
    /// installs, and managed llama-server processes. Its provider is registered
    /// in [`Self::models`] as `local`.
    pub fn inference(&self) -> &LocalInference {
        &self.inference
    }
}

/// Open local inference and register its `local` provider. A configured
/// provider that already uses the id `local` keeps it (local models are then
/// reachable only through the `model.*` local methods).
fn open_inference(
    paths: &DrivePaths,
    models: &ModelServices,
    events: &EventLog,
) -> Result<LocalInference, ServicesError> {
    let inference = LocalInference::open(
        paths,
        Sandbox::locate(),
        Arc::new(events.clone()),
        InferenceOptions::default(),
    )
    .map_err(|error| ServicesError::new("inference", error.code(), error.to_string()))?;
    match models
        .registry()
        .register_shared(Arc::new(inference.provider()))
    {
        Ok(()) | Err(RegistryError::Duplicate(_)) => Ok(inference),
        Err(error) => Err(ServicesError::new(
            "inference",
            error.code(),
            error.to_string(),
        )),
    }
}

impl Drop for Services {
    /// Stop managed model servers with the daemon.
    fn drop(&mut self) {
        self.inference.shutdown();
    }
}

/// Open the drive's event log and import PR #40's superseded log once, if present.
fn open_events(paths: &DrivePaths, legacy: &Path) -> Result<EventLog, ServicesError> {
    let events_error = |error: aidex_events::EventError| {
        ServicesError::new("events", error.code(), error.to_string())
    };
    paths
        .ensure_dir(&paths.sessions())
        .map_err(|error| ServicesError::new("drive", error.code(), error.to_string()))?;
    let events = EventLog::open(event_log_path(paths), event_retention()).map_err(events_error)?;
    endpoint::refuse_symlink(legacy)
        .map_err(|error| ServicesError::new("events", error.code(), error.to_string()))?;
    if legacy.is_file() {
        events.import_legacy(legacy).map_err(events_error)?;
    }
    Ok(events)
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
