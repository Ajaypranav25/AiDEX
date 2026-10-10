//! Starting and stopping `aidexd` for the session drive.
//!
//! `start` launches the `aidexd` executable shipped next to the app executable (falling
//! back to `PATH`) through `aidexd::lifecycle::launch`, which recovers a provably stale
//! endpoint, spawns `aidexd run --drive <root>` detached, and waits for an
//! authenticated `daemon.status`. The app remembers whether it started the daemon and
//! stops it on exit only in that case; a daemon that was already running is left alone.

use crate::error::{kind, BridgeError};
use aidex_core::ServiceErrorCode;
use aidexd::{
    lifecycle::{self, STOP_TIMEOUT},
    methods::daemon::DaemonStatus,
    DaemonError,
};
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

/// How long app exit waits for a daemon it started to stop.
pub const EXIT_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// File name of the daemon executable on this platform.
pub fn daemon_file_name() -> &'static str {
    if cfg!(windows) {
        "aidexd.exe"
    } else {
        "aidexd"
    }
}

/// `aidexd` next to `executable`, else the first match on `path_var`.
pub fn locate_daemon(executable: Option<&Path>, path_var: Option<&OsStr>) -> Option<PathBuf> {
    let name = daemon_file_name();
    let beside = executable
        .and_then(Path::parent)
        .map(|directory| directory.join(name));
    beside
        .into_iter()
        .chain(
            path_var
                .map(|value| {
                    env::split_paths(value)
                        .map(|directory| directory.join(name))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
        .find(|candidate| candidate.is_file())
}

/// Outcome of a start request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Started {
    /// This app launched the daemon and will stop it on exit.
    Launched(DaemonStatus),
    /// A daemon was already serving the drive; it is left running on exit.
    AlreadyRunning { pid: u32 },
}

/// Tracks the daemon this app launched, if any.
#[derive(Default)]
pub struct DaemonSupervisor {
    owned: Mutex<Option<PathBuf>>,
}

impl DaemonSupervisor {
    /// Launch the daemon for `root` with the executable located by [`locate_daemon`].
    pub fn start(&self, root: &Path) -> Result<Started, BridgeError> {
        let executable = env::current_exe().ok();
        let path_var = env::var_os("PATH");
        let daemon =
            locate_daemon(executable.as_deref(), path_var.as_deref()).ok_or_else(|| {
                BridgeError::new(
                    ServiceErrorCode::NotFound,
                    kind::DAEMON_NOT_FOUND,
                    format!(
                        "cannot find `{}` next to the AiDEX app or on PATH",
                        daemon_file_name()
                    ),
                )
            })?;
        self.start_with(root, &daemon)
    }

    /// Launch `daemon` for `root`.
    pub fn start_with(&self, root: &Path, daemon: &Path) -> Result<Started, BridgeError> {
        match lifecycle::launch(root, daemon) {
            Ok(status) => {
                self.set_owned(Some(root.to_path_buf()));
                Ok(Started::Launched(status))
            }
            Err(DaemonError::AlreadyRunning { pid }) => Ok(Started::AlreadyRunning { pid }),
            Err(error) => Err(error.into()),
        }
    }

    /// Ask the daemon serving `root` to stop and wait until it has.
    pub fn stop(&self, root: &Path) -> Result<(), BridgeError> {
        lifecycle::stop(root, STOP_TIMEOUT)?;
        if self.owns(root) {
            self.set_owned(None);
        }
        Ok(())
    }

    /// Whether this app launched the daemon serving `root`.
    pub fn owns(&self, root: &Path) -> bool {
        self.owned
            .lock()
            .is_ok_and(|owned| owned.as_deref() == Some(root))
    }

    /// On exit: stop the daemon only if this app launched it. Best effort.
    pub fn stop_owned(&self) -> Option<Result<(), BridgeError>> {
        let root = self.owned.lock().ok().and_then(|mut owned| owned.take())?;
        Some(lifecycle::stop(&root, EXIT_STOP_TIMEOUT).map_err(BridgeError::from))
    }

    fn set_owned(&self, root: Option<PathBuf>) {
        if let Ok(mut owned) = self.owned.lock() {
            *owned = root;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn the_daemon_beside_the_app_wins_over_path() {
        let app = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let executable = app.path().join("aidex-desktop.exe");
        let beside = app.path().join(daemon_file_name());
        let on_path = elsewhere.path().join(daemon_file_name());
        fs::write(&on_path, b"").unwrap();
        let path_var = env::join_paths([elsewhere.path()]).unwrap();

        assert_eq!(
            locate_daemon(Some(&executable), Some(&path_var)),
            Some(on_path.clone()),
            "falls back to PATH"
        );
        fs::write(&beside, b"").unwrap();
        assert_eq!(
            locate_daemon(Some(&executable), Some(&path_var)),
            Some(beside)
        );
        assert_eq!(locate_daemon(None, Some(&path_var)), Some(on_path));
        assert_eq!(locate_daemon(None, None), None);
    }

    #[test]
    fn directories_named_like_the_daemon_are_ignored() {
        let app = tempfile::tempdir().unwrap();
        fs::create_dir(app.path().join(daemon_file_name())).unwrap();
        let executable = app.path().join("aidex-desktop.exe");
        assert_eq!(locate_daemon(Some(&executable), None), None);
    }

    #[test]
    fn an_app_that_launched_nothing_stops_nothing_on_exit() {
        let supervisor = DaemonSupervisor::default();
        assert!(supervisor.stop_owned().is_none());
        let root = tempfile::tempdir().unwrap();
        assert!(!supervisor.owns(root.path()));
    }

    #[test]
    fn stopping_without_a_daemon_reports_unavailable_and_keeps_ownership_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let supervisor = DaemonSupervisor::default();
        let error = supervisor.stop(root.path()).unwrap_err();
        assert_eq!(error.kind, kind::DAEMON_UNAVAILABLE);
        assert!(!supervisor.owns(root.path()));
    }

    #[test]
    fn a_missing_daemon_executable_fails_to_launch() {
        let root = tempfile::tempdir().unwrap();
        aidex_drive::init_with(
            root.path(),
            aidex_drive::InitOptions {
                require_removable: false,
            },
        )
        .unwrap();
        let supervisor = DaemonSupervisor::default();
        let missing = root.path().join("no-such-aidexd.exe");
        let error = supervisor.start_with(root.path(), &missing).unwrap_err();
        assert_eq!(error.code, "unavailable");
        assert!(!supervisor.owns(root.path()));
    }
}
