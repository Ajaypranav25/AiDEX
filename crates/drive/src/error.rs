//! Errors returned by drive resolution, initialization, migration, and path services.

use std::{error::Error, fmt, path::PathBuf};

use aidex_core::ServiceErrorCode;

/// Errors from drive resolution, initialization, or health checks.
#[derive(Debug)]
pub enum DriveError {
    InvalidRoot(String),
    NotRemovable(String),
    ReadOnly(String),
    LowSpace {
        available: u64,
        required: u64,
    },
    MissingManifest(PathBuf),
    CorruptManifest(PathBuf, String),
    Io(String),
    Discovery(String),
    /// The drive at this root still uses manifest schema v1; `aidex init` migrates it.
    MigrationRequired(PathBuf),
    /// The v1 backup path already holds different content; nothing was changed.
    MigrationConflict(PathBuf),
    /// Migration stopped before completion; the reason says what was preserved.
    MigrationFailed(String),
    /// The path is outside the drive or not beneath a declared drive directory.
    UnmanagedPath(PathBuf),
    /// A path component is a symbolic link, junction, or non-directory.
    UnsafeComponent(PathBuf),
}

impl DriveError {
    /// Stable service code for API mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::InvalidRoot(_) | Self::NotRemovable(_) | Self::UnmanagedPath(_) => {
                ServiceErrorCode::InvalidArgument
            }
            Self::MissingManifest(_) => ServiceErrorCode::NotFound,
            Self::ReadOnly(_) | Self::UnsafeComponent(_) => ServiceErrorCode::PermissionDenied,
            Self::CorruptManifest(..) | Self::MigrationConflict(_) => ServiceErrorCode::Conflict,
            Self::MigrationRequired(_) => ServiceErrorCode::IncompatibleVersion,
            Self::LowSpace { .. } | Self::Discovery(_) => ServiceErrorCode::Unavailable,
            Self::Io(_) | Self::MigrationFailed(_) => ServiceErrorCode::Internal,
        }
    }
}

impl fmt::Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoot(message) => write!(f, "invalid drive root: {message}"),
            Self::NotRemovable(root) => {
                write!(f, "target is not a recognized removable drive: {root}")
            }
            Self::ReadOnly(root) => write!(f, "drive is not writable: {root}"),
            Self::LowSpace {
                available,
                required,
            } => write!(
                f,
                "drive has {available} bytes free; at least {required} bytes are required"
            ),
            Self::MissingManifest(path) => write!(
                f,
                "AiDEX manifest is missing: {} (run `aidex init --drive <path>`)",
                path.display()
            ),
            Self::CorruptManifest(path, reason) => write!(
                f,
                "AiDEX manifest is corrupt or unsupported: {} ({reason})",
                path.display()
            ),
            Self::Io(message) | Self::Discovery(message) => f.write_str(message),
            Self::MigrationRequired(root) => write!(
                f,
                "AiDEX drive {root} uses manifest schema v1; run `aidex init --drive {root}` to migrate it to schema v2",
                root = root.display()
            ),
            Self::MigrationConflict(backup) => write!(
                f,
                "cannot migrate the schema v1 manifest: backup `{}` already exists with different content; nothing was changed (move that file aside after confirming it is not needed, then retry)",
                backup.display()
            ),
            Self::MigrationFailed(reason) => write!(f, "manifest migration failed: {reason}"),
            Self::UnmanagedPath(path) => write!(
                f,
                "refusing to use `{}`: it is not a declared AiDEX drive directory or beneath one",
                path.display()
            ),
            Self::UnsafeComponent(path) => write!(
                f,
                "refusing to use non-directory or symbolic link `{}`",
                path.display()
            ),
        }
    }
}

impl Error for DriveError {}
