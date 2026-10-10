//! Launcher failures, their stable exit codes, and actionable hints.

use std::{fmt, io, path::Path};

use aidex_bundle::{BundleError, BundleErrorKind};
use aidex_drive::DriveError;

/// Exit codes. They match the `aidex` CLI where the meaning overlaps (2 usage, 4 drive,
/// 8 not found, 10 incompatible, 11 integrity).
pub mod exit {
    pub const FAILURE: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const DRIVE: i32 = 4;
    pub const NOT_INSTALLED: i32 = 8;
    pub const INCOMPATIBLE: i32 = 10;
    /// The installed bundle failed integrity or signature verification; nothing was run.
    pub const INTEGRITY: i32 = 11;
    /// The CLI could not be executed (for example a `noexec` mount).
    pub const NOT_EXECUTABLE: i32 = 126;
}

/// A launcher failure: what happened, how to fix it, and the exit code.
#[derive(Debug)]
pub struct LaunchError {
    pub code: i32,
    pub message: String,
    pub hint: Option<String>,
}

impl LaunchError {
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            code: exit::USAGE,
            message: message.into(),
            hint: Some(
                "AiDEX.exe passes its arguments to `aidex`; launcher options are `--launcher-info` and `--launcher-verify`".into(),
            ),
        }
    }

    pub fn no_drive(exe: &Path) -> Self {
        Self {
            code: exit::DRIVE,
            message: format!(
                "`{}` is not on an initialized AiDEX drive (no aidex-drive.json found above it)",
                exe.display()
            ),
            hint: Some(
                "run the launcher from the drive root; initialize a drive with `aidex init --drive <root>`".into(),
            ),
        }
    }

    pub fn misplaced(exe: &Path, root: &Path) -> Self {
        Self {
            code: exit::DRIVE,
            message: format!(
                "`{}` is not at a launcher location of the drive at `{}`",
                exe.display(),
                root.display()
            ),
            hint: Some("use AiDEX.exe (Windows) or ./aidex (Linux/macOS) at the drive root".into()),
        }
    }

    pub fn io(context: impl fmt::Display, error: io::Error) -> Self {
        Self {
            code: exit::FAILURE,
            message: format!("{context}: {error}"),
            hint: None,
        }
    }

    /// The trust store or the drive's revocation list could not be read: fail closed.
    pub fn trust(error: BundleError) -> Self {
        Self {
            code: exit::INTEGRITY,
            message: error.to_string(),
            hint: Some(
                "nothing was run; fix or remove manifests/revoked-keys.json only if you know why it is invalid".into(),
            ),
        }
    }

    /// The CLI could not be started.
    pub fn spawn(program: &Path, error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::PermissionDenied {
            return Self {
                code: exit::NOT_EXECUTABLE,
                message: format!("cannot execute `{}`: {error}", program.display()),
                hint: Some(
                    "the drive may be mounted without execute permission (for example FAT32 with `showexec`, or `noexec`); remount it with exec".into(),
                ),
            };
        }
        Self::io(format_args!("cannot start `{}`", program.display()), error)
    }

    pub fn report(&self) {
        eprintln!("AiDEX launcher: {}", self.message);
        if let Some(hint) = &self.hint {
            eprintln!("hint: {hint}");
        }
    }
}

const INSTALL_COMMAND: &str = "`aidex-packager install --bundle <archive> --drive <drive root>`";

impl From<BundleError> for LaunchError {
    fn from(error: BundleError) -> Self {
        let (code, hint) = match error.kind() {
            BundleErrorKind::Drive => (exit::DRIVE, None),
            BundleErrorKind::NotInstalled => (
                exit::NOT_INSTALLED,
                Some(format!("install a bundle with {INSTALL_COMMAND}")),
            ),
            BundleErrorKind::Incompatible => (
                exit::INCOMPATIBLE,
                Some("install the bundle built for this host's platform on this drive".into()),
            ),
            BundleErrorKind::Integrity => (
                exit::INTEGRITY,
                Some(format!(
                    "nothing was run; the installed bundle may be corrupted or modified, so reinstall it with {INSTALL_COMMAND}"
                )),
            ),
            BundleErrorKind::Untrusted => (
                exit::INTEGRITY,
                Some(format!(
                    "nothing was run; install a bundle signed by a key this launcher trusts (`aidex update apply <archive>` or {INSTALL_COMMAND}); for an unsigned development bundle only, set AIDEX_ALLOW_UNSIGNED=1"
                )),
            ),
            BundleErrorKind::Occupied
            | BundleErrorKind::InvalidInput
            | BundleErrorKind::Aborted
            | BundleErrorKind::Io => (exit::FAILURE, None),
        };
        Self {
            code,
            message: error.to_string(),
            hint,
        }
    }
}

impl From<DriveError> for LaunchError {
    fn from(error: DriveError) -> Self {
        let hint = match error {
            DriveError::MigrationRequired(_) => {
                Some("migrate the drive with `aidex init --drive <root>`".to_owned())
            }
            _ => None,
        };
        Self {
            code: exit::DRIVE,
            message: error.to_string(),
            hint,
        }
    }
}
