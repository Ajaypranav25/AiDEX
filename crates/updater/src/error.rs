//! Update failures with stable reasons (`data.reason` on the API) and service codes.

use std::{fmt, io, path::PathBuf};

use aidex_bundle::{
    BundleError, BundleErrorKind, IntegrityProblem, JournalState, SignatureError, Target,
};
use aidex_core::ServiceErrorCode;

use crate::Checkpoint;

/// Why an update, check, rollback, or recovery did not complete.
#[derive(Debug)]
pub enum UpdateError {
    /// Another update or rollback holds the drive's update lock.
    Busy {
        holder: String,
    },
    /// A previous operation did not finish; recovery must run first.
    RecoveryRequired {
        state: JournalState,
    },
    /// This host has no bundle target name.
    UnsupportedHost,
    /// No bundle is installed for the target.
    NotInstalled {
        target: Target,
    },
    /// There is no previous bundle to roll back to.
    NoPrevious {
        target: Target,
    },
    /// The update source does not exist.
    SourceNotFound {
        path: PathBuf,
    },
    /// The update source is not an update package.
    InvalidSource(String),
    /// A release directory holds no package for the target.
    NoPackage {
        dir: PathBuf,
        target: Target,
    },
    /// `SHA256SUMS` next to the package lists a different digest.
    ChecksumMismatch {
        file: String,
        expected: String,
        actual: String,
    },
    /// `SHA256SUMS` next to the package does not list it.
    ChecksumMissing {
        file: String,
    },
    /// The package or its contents exceed the configured limits.
    ArchiveTooLarge {
        detail: String,
    },
    /// An archive entry could escape or confuse extraction.
    UnsafeArchive {
        entry: String,
        reason: String,
    },
    /// The package is unsigned (not allowed) or its signature is not acceptable.
    Signature(SignatureError),
    /// A file of the package does not match its manifest.
    Tampered {
        path: String,
        problem: IntegrityProblem,
    },
    /// A critical file the manifest lists (launcher, CLI, daemon, helper, runtime) is
    /// missing from the package.
    MissingCriticalFile {
        path: String,
    },
    WrongTarget {
        expected: Target,
        found: Target,
    },
    /// The package is older than the active bundle.
    Downgrade {
        current: String,
        candidate: String,
    },
    /// `allow_downgrade` was given, but the package is not the recorded previous bundle.
    DowngradeNotPrevious {
        candidate: String,
        previous: Option<String>,
    },
    /// The package has the active version but a different build, and the active
    /// bundle is intact (nothing to repair).
    NotNewer {
        current: String,
        candidate: String,
    },
    /// A version is not SemVer, so it cannot be ordered.
    InvalidVersion {
        version: String,
    },
    IncompatibleDriveSchema {
        required: u32,
        found: u32,
    },
    /// The package requires extraction to the host, which AiDEX never does.
    HostExtraction,
    /// A staged binary failed the self-test; nothing was switched.
    SelfTestFailed {
        command: String,
        detail: String,
    },
    /// Any other bundle, pointer, journal, or drive failure.
    Bundle(BundleError),
    /// A fault-injection hook simulated power loss at this checkpoint (tests only):
    /// nothing was cleaned up, exactly as after a crash.
    Interrupted(Checkpoint),
    Io {
        context: String,
        error: io::Error,
    },
}

impl UpdateError {
    pub(crate) fn io(context: impl Into<String>, error: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            error,
        }
    }

    /// Map a bundle error raised while checking a package to its typed refusal.
    pub(crate) fn from_package(error: BundleError, critical: &[String]) -> Self {
        match error {
            BundleError::Signature { error, .. } => Self::Signature(error),
            BundleError::Integrity {
                path,
                problem: IntegrityProblem::Missing,
                ..
            } if critical.contains(&path) => Self::MissingCriticalFile { path },
            BundleError::Integrity { path, problem, .. } => Self::Tampered { path, problem },
            BundleError::WrongTarget { expected, found } => Self::WrongTarget { expected, found },
            BundleError::DriveSchemaTooOld { required, found } => {
                Self::IncompatibleDriveSchema { required, found }
            }
            BundleError::HostExtractionUnsupported => Self::HostExtraction,
            BundleError::ArchiveEntry { name, reason } => Self::UnsafeArchive {
                entry: name,
                reason,
            },
            BundleError::ArchiveLimit(detail) => Self::ArchiveTooLarge { detail },
            other => Self::Bundle(other),
        }
    }

    /// Stable machine-readable reason (API `data.reason`, `update.failed.code`).
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Busy { .. } => "busy",
            Self::RecoveryRequired { .. } => "recovery_required",
            Self::UnsupportedHost => "unsupported_host",
            Self::NotInstalled { .. } => "not_installed",
            Self::NoPrevious { .. } => "no_previous",
            Self::SourceNotFound { .. } => "source_not_found",
            Self::InvalidSource(_) => "invalid_source",
            Self::NoPackage { .. } => "no_package",
            Self::ChecksumMismatch { .. } => "checksum_mismatch",
            Self::ChecksumMissing { .. } => "checksum_missing",
            Self::ArchiveTooLarge { .. } => "archive_too_large",
            Self::UnsafeArchive { .. } => "unsafe_archive",
            Self::Signature(error) => error.reason(),
            Self::Tampered { .. } => "tampered",
            Self::MissingCriticalFile { .. } => "missing_critical_file",
            Self::WrongTarget { .. } => "wrong_target",
            Self::Downgrade { .. } => "downgrade",
            Self::DowngradeNotPrevious { .. } => "downgrade_not_previous",
            Self::NotNewer { .. } => "not_newer",
            Self::InvalidVersion { .. } => "invalid_version",
            Self::IncompatibleDriveSchema { .. } => "incompatible_drive_schema",
            Self::HostExtraction => "host_extraction",
            Self::SelfTestFailed { .. } => "self_test_failed",
            Self::Bundle(error) => match error.kind() {
                BundleErrorKind::Drive => "drive",
                BundleErrorKind::NotInstalled => "not_installed",
                BundleErrorKind::Incompatible => "incompatible",
                BundleErrorKind::Integrity => "integrity",
                BundleErrorKind::Untrusted => "untrusted",
                BundleErrorKind::Occupied => "occupied",
                BundleErrorKind::InvalidInput => "invalid_source",
                BundleErrorKind::Aborted => "aborted",
                BundleErrorKind::Io => "io",
            },
            Self::Interrupted(_) => "interrupted",
            Self::Io { .. } => "io",
        }
    }

    /// Whether this is an authenticity or integrity refusal (CLI exit 11).
    pub fn is_integrity(&self) -> bool {
        match self {
            Self::ChecksumMismatch { .. }
            | Self::ChecksumMissing { .. }
            | Self::UnsafeArchive { .. }
            | Self::Signature(_)
            | Self::Tampered { .. }
            | Self::MissingCriticalFile { .. } => true,
            Self::Bundle(error) => matches!(
                error.kind(),
                BundleErrorKind::Integrity | BundleErrorKind::Untrusted
            ),
            _ => false,
        }
    }

    /// Stable service code for API mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Bundle(error) => error.code(),
            Self::NotInstalled { .. }
            | Self::NoPrevious { .. }
            | Self::SourceNotFound { .. }
            | Self::NoPackage { .. } => ServiceErrorCode::NotFound,
            Self::InvalidSource(_) | Self::InvalidVersion { .. } | Self::ArchiveTooLarge { .. } => {
                ServiceErrorCode::InvalidArgument
            }
            Self::UnsupportedHost
            | Self::WrongTarget { .. }
            | Self::Downgrade { .. }
            | Self::DowngradeNotPrevious { .. }
            | Self::NotNewer { .. }
            | Self::IncompatibleDriveSchema { .. }
            | Self::HostExtraction => ServiceErrorCode::IncompatibleVersion,
            Self::Busy { .. }
            | Self::RecoveryRequired { .. }
            | Self::ChecksumMismatch { .. }
            | Self::ChecksumMissing { .. }
            | Self::UnsafeArchive { .. }
            | Self::Signature(_)
            | Self::Tampered { .. }
            | Self::MissingCriticalFile { .. }
            | Self::SelfTestFailed { .. } => ServiceErrorCode::Conflict,
            Self::Interrupted(_) => ServiceErrorCode::Unavailable,
            Self::Io { .. } => ServiceErrorCode::Internal,
        }
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy { holder } => write!(
                f,
                "another update or rollback is in progress on this drive ({holder})"
            ),
            Self::RecoveryRequired { state } => write!(
                f,
                "an earlier update stopped during `{}`; it is recovered when the daemon starts (restart the daemon, then retry)",
                state.as_str()
            ),
            Self::UnsupportedHost => f.write_str("this host platform has no AiDEX bundle target"),
            Self::NotInstalled { target } => {
                write!(f, "no AiDEX bundle for {target} is installed on this drive")
            }
            Self::NoPrevious { target } => {
                write!(f, "there is no previous {target} bundle to roll back to")
            }
            Self::SourceNotFound { path } => {
                write!(f, "update source `{}` does not exist", path.display())
            }
            Self::InvalidSource(reason) => write!(f, "invalid update source: {reason}"),
            Self::NoPackage { dir, target } => write!(
                f,
                "`{}` holds no aidex-<version>-{target} package",
                dir.display()
            ),
            Self::ChecksumMismatch {
                file,
                expected,
                actual,
            } => write!(
                f,
                "`{file}` has sha256 {actual}, but SHA256SUMS lists {expected}"
            ),
            Self::ChecksumMissing { file } => {
                write!(f, "SHA256SUMS next to the package does not list `{file}`")
            }
            Self::ArchiveTooLarge { detail } => {
                write!(f, "the update package is too large: {detail}")
            }
            Self::UnsafeArchive { entry, reason } => {
                write!(
                    f,
                    "the update package has an unsafe entry `{entry}`: {reason}"
                )
            }
            Self::Signature(error) => write!(f, "the update package is not trusted: {error}"),
            Self::Tampered { path, problem } => write!(
                f,
                "the update package does not match its manifest at `{path}`: {problem}"
            ),
            Self::MissingCriticalFile { path } => write!(
                f,
                "the update package lacks the critical file `{path}` its manifest lists"
            ),
            Self::WrongTarget { expected, found } => write!(
                f,
                "the update package is for {found}, but this drive's update target is {expected}"
            ),
            Self::Downgrade { current, candidate } => write!(
                f,
                "the update package ({candidate}) is older than the active bundle ({current}); use `--allow-downgrade` only to return to the recorded previous bundle"
            ),
            Self::DowngradeNotPrevious {
                candidate,
                previous,
            } => match previous {
                Some(previous) => write!(
                    f,
                    "downgrades may only return to the recorded previous bundle ({previous}), not {candidate}"
                ),
                None => write!(
                    f,
                    "downgrades may only return to the recorded previous bundle, and there is none ({candidate} refused)"
                ),
            },
            Self::NotNewer { current, candidate } => write!(
                f,
                "the update package ({candidate}) is not newer than the intact active bundle ({current})"
            ),
            Self::InvalidVersion { version } => {
                write!(f, "version `{version}` is not a SemVer version")
            }
            Self::IncompatibleDriveSchema { required, found } => write!(
                f,
                "the update needs drive schema v{required}, but this drive uses v{found}"
            ),
            Self::HostExtraction => f.write_str(
                "the update package requires extraction to the host, which AiDEX does not do",
            ),
            Self::SelfTestFailed { command, detail } => {
                write!(f, "the staged `{command}` failed its self-test: {detail}")
            }
            Self::Bundle(error) => write!(f, "{error}"),
            Self::Interrupted(checkpoint) => {
                write!(f, "interrupted at `{}` (simulated)", checkpoint.as_str())
            }
            Self::Io { context, error } => write!(f, "{context}: {error}"),
        }
    }
}

impl std::error::Error for UpdateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bundle(error) => Some(error),
            Self::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl From<BundleError> for UpdateError {
    fn from(error: BundleError) -> Self {
        Self::Bundle(error)
    }
}

impl From<aidex_drive::DriveError> for UpdateError {
    fn from(error: aidex_drive::DriveError) -> Self {
        Self::Bundle(BundleError::Drive(error))
    }
}
