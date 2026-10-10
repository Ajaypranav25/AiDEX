//! Report shapes returned by the status commands, built without Tauri so they are
//! unit-tested directly. The TypeScript mirrors live in `packages/client/src/dto/`.

use crate::{
    error::{kind, BridgeError},
    lifecycle::drive::{display_path, DriveResolution, DriveSource},
    state::AppState,
};
use aidex_drive::{
    status_with, volume_for, DriveStatus, StatusOptions, VolumeInfo, CURRENT_SCHEMA_VERSION,
};
use aidexd::{methods::daemon::DaemonStatus, ErrorCode};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// `aidex_status`: the session drive and whether its daemon answers.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AidexStatus {
    pub app_version: &'static str,
    pub drive: DriveState,
    pub daemon: DaemonState,
}

/// The session drive.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DriveState {
    Resolved {
        root: String,
        source: DriveSource,
    },
    Invalid {
        source: DriveSource,
        path: String,
        error: BridgeError,
    },
    None,
}

/// Daemon reachability for the session drive.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DaemonState {
    Running {
        status: DaemonStatus,
        /// This app launched the daemon and will stop it on exit.
        started_by_app: bool,
    },
    /// No daemon answers for the drive (offer to start it).
    Stopped {
        error: BridgeError,
    },
    /// The daemon speaks another IPC contract version.
    Incompatible {
        error: BridgeError,
    },
    /// The endpoint capability was rejected for this user.
    Unauthorized {
        error: BridgeError,
    },
    Error {
        error: BridgeError,
    },
    /// There is no drive to run a daemon for.
    NoDrive,
}

impl DaemonState {
    /// Classify a `daemon.status` outcome.
    pub fn from_outcome(outcome: Result<DaemonStatus, BridgeError>, started_by_app: bool) -> Self {
        match outcome {
            Ok(status) => Self::Running {
                status,
                started_by_app,
            },
            Err(error) if error.kind == kind::DAEMON_UNAVAILABLE => Self::Stopped { error },
            Err(error) if error.kind == ErrorCode::IncompatibleVersion.name() => {
                Self::Incompatible { error }
            }
            Err(error) if error.kind == ErrorCode::Unauthorized.name() => {
                Self::Unauthorized { error }
            }
            Err(error) => Self::Error { error },
        }
    }
}

/// `drive_status`: manifest health plus the volume the drive lives on.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DriveReport {
    pub root: String,
    pub source: DriveSource,
    pub health: Option<DriveHealth>,
    pub health_error: Option<BridgeError>,
    pub volume: Option<VolumeInfo>,
    pub volume_error: Option<BridgeError>,
}

/// `aidex_drive::DriveStatus` in UI terms.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DriveHealth {
    pub writable: bool,
    pub free_bytes: Option<u64>,
    pub schema_version: Option<u32>,
    pub current_schema_version: u32,
    pub migration_required: bool,
    pub migration_advice: Option<String>,
}

impl From<&DriveStatus> for DriveHealth {
    fn from(status: &DriveStatus) -> Self {
        Self {
            writable: status.writable,
            free_bytes: status.free_bytes,
            schema_version: status.schema_version,
            current_schema_version: CURRENT_SCHEMA_VERSION,
            migration_required: status.migration_required(),
            migration_advice: status.migration_advice(),
        }
    }
}

/// Collect [`AidexStatus`] for the session.
pub fn aidex_status(state: &AppState) -> AidexStatus {
    let (drive, daemon) = match state.drive.resolve() {
        DriveResolution::Resolved(drive) => {
            let outcome = state
                .bridge
                .connection
                .call(&drive.root, "daemon.status", Value::Null)
                .and_then(|value| {
                    serde_json::from_value::<DaemonStatus>(value).map_err(|error| {
                        BridgeError::internal(format!("unexpected daemon.status result: {error}"))
                    })
                });
            let daemon = DaemonState::from_outcome(outcome, state.daemon.owns(&drive.root));
            (
                DriveState::Resolved {
                    root: display_path(&drive.root),
                    source: drive.source,
                },
                daemon,
            )
        }
        DriveResolution::Invalid {
            source,
            path,
            error,
        } => (
            DriveState::Invalid {
                source,
                path: display_path(&path),
                error,
            },
            DaemonState::NoDrive,
        ),
        DriveResolution::None => (DriveState::None, DaemonState::NoDrive),
    };
    AidexStatus {
        app_version: env!("CARGO_PKG_VERSION"),
        drive,
        daemon,
    }
}

/// Build a [`DriveReport`] for `root`. Never modifies the drive.
pub fn drive_report(root: &Path, source: DriveSource) -> DriveReport {
    let health = status_with(
        root,
        StatusOptions {
            require_removable: false,
        },
    );
    let volume = volume_for(root);
    DriveReport {
        root: display_path(root),
        source,
        health: health.as_ref().ok().map(DriveHealth::from),
        health_error: health.err().map(BridgeError::from),
        volume: volume.as_ref().ok().cloned(),
        volume_error: volume.err().map(BridgeError::from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aidexd::methods::daemon::DaemonState as Ready;

    fn status() -> DaemonStatus {
        DaemonStatus {
            state: Ready::Ready,
            pid: 42,
            contract_version: 2,
            version: "0.1.0".into(),
            drive_root: r"\\?\H:\".into(),
            uptime_ms: 1_000,
        }
    }

    #[test]
    fn daemon_outcomes_map_to_ui_states() {
        assert_eq!(
            DaemonState::from_outcome(Ok(status()), true),
            DaemonState::Running {
                status: status(),
                started_by_app: true
            }
        );
        let stopped = DaemonState::from_outcome(
            Err(aidexd::ClientError::Unavailable("no endpoint".into()).into()),
            false,
        );
        assert!(matches!(stopped, DaemonState::Stopped { .. }));
        let incompatible = DaemonState::from_outcome(
            Err(aidexd::ClientError::Incompatible {
                message: "v1".into(),
                supported: vec![1],
            }
            .into()),
            false,
        );
        assert!(matches!(incompatible, DaemonState::Incompatible { .. }));
        let unauthorized = DaemonState::from_outcome(
            Err(aidexd::ClientError::Unauthorized("capability".into()).into()),
            false,
        );
        assert!(matches!(unauthorized, DaemonState::Unauthorized { .. }));
        let other = DaemonState::from_outcome(Err(BridgeError::internal("boom")), false);
        assert!(matches!(other, DaemonState::Error { .. }));
    }

    #[test]
    fn status_serializes_with_tagged_states() {
        let value = serde_json::to_value(AidexStatus {
            app_version: "0.1.0",
            drive: DriveState::Resolved {
                root: r"H:\".into(),
                source: DriveSource::Executable,
            },
            daemon: DaemonState::Running {
                status: status(),
                started_by_app: false,
            },
        })
        .unwrap();
        assert_eq!(value["drive"]["state"], "resolved");
        assert_eq!(value["drive"]["source"], "executable");
        assert_eq!(value["daemon"]["state"], "running");
        assert_eq!(value["daemon"]["status"]["pid"], 42);
        assert_eq!(value["daemon"]["started_by_app"], false);
        assert_eq!(
            serde_json::to_value(DaemonState::NoDrive).unwrap()["state"],
            "no_drive"
        );
    }

    #[test]
    fn a_folder_without_a_manifest_reports_the_error_instead_of_health() {
        let root = tempfile::tempdir().unwrap();
        let report = drive_report(root.path(), DriveSource::Environment);
        assert!(report.health.is_none());
        assert_eq!(report.health_error.unwrap().kind, "missing_manifest");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_selected_developer_drive_reports_stopped_daemon_and_healthy_manifest() {
        let root = tempfile::tempdir().unwrap();
        aidex_drive::init_with(
            root.path(),
            aidex_drive::InitOptions {
                require_removable: false,
            },
        )
        .unwrap();
        let state = AppState::default();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        state.drive.select(canonical.clone());

        let report = aidex_status(&state);
        assert_eq!(
            report.drive,
            DriveState::Resolved {
                root: display_path(&canonical),
                source: DriveSource::Selected
            }
        );
        assert!(matches!(report.daemon, DaemonState::Stopped { .. }));

        let drive = drive_report(&canonical, DriveSource::Selected);
        let health = drive.health.expect("a developer folder reports health");
        assert_eq!(health.schema_version, Some(CURRENT_SCHEMA_VERSION));
        assert!(health.writable);
        assert!(!health.migration_required);
        assert!(drive.health_error.is_none());
        if let Some(volume) = drive.volume {
            assert!(
                !volume.is_eligible(),
                "temporary folders are on internal disks"
            );
        }
    }
}
