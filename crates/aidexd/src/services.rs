//! Daemon-wide service composition.
//!
//! [`Services`] is built once at startup and shared with handlers as
//! `Arc<Services>`. Domain services (events, workspaces, sessions, vault, ...)
//! are added here as fields constructed in [`Services::open`]; their JSON-RPC
//! adapters live in `methods/<domain>.rs`.

use crate::{endpoint, lifecycle::ShutdownSignal, methods::events::event_error_code};
use aidex_core::ServiceErrorCode;
use aidex_events::EventLog;
use std::{
    fmt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Event database, relative to the drive's `.aidex/runtime` directory.
pub const EVENT_FILE: &str = "events.sqlite";
/// Events retained per workspace before the oldest expire.
pub const EVENT_RETENTION: u64 = 10_000;

pub struct Services {
    drive_root: PathBuf,
    pid: u32,
    started_at: Instant,
    shutdown: ShutdownSignal,
    events: EventLog,
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
        let state_error = |error: endpoint::EndpointError| {
            ServicesError::new("daemon", error.code(), error.to_string())
        };
        let runtime = endpoint::ensure_state_dir(drive_root).map_err(state_error)?;
        let event_path = runtime.join(EVENT_FILE);
        endpoint::refuse_symlink(&event_path).map_err(state_error)?;
        let events = EventLog::open(&event_path, EVENT_RETENTION).map_err(|error| {
            ServicesError::new("events", event_error_code(&error), error.to_string())
        })?;
        Ok(Self {
            drive_root: drive_root.to_path_buf(),
            pid: std::process::id(),
            started_at: Instant::now(),
            shutdown,
            events,
        })
    }

    pub fn drive_root(&self) -> &Path {
        &self.drive_root
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
