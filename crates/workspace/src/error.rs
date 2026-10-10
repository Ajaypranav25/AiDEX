//! Workspace service errors with stable service codes and machine-readable reasons.

use std::{error::Error, fmt, io, path::PathBuf};

use aidex_core::{ServiceErrorCode, WorkspacePath};
use aidex_drive::DriveError;

/// Every failure the workspace service reports. Paths in messages are the caller's
/// own inputs or AiDEX-managed locations; no file contents are ever included.
#[derive(Debug)]
pub enum WorkspaceError {
    /// A path argument is malformed, relative where an absolute path is required,
    /// or uses a platform-reserved spelling (device names, trailing dots/spaces).
    InvalidPath { path: String, reason: String },
    /// A workspace name is empty, too long, or contains unsupported characters.
    InvalidName(String),
    /// No workspace with this id or name is registered.
    NotFound(String),
    /// A workspace-relative path does not exist (read/write access).
    PathNotFound(WorkspacePath),
    /// The workspace root is not present on this host. It is never recreated.
    RootUnavailable { workspace: String, reason: String },
    /// The request conflicts with registry state (duplicate root or name, children).
    Conflict(String),
    /// Resolution left the workspace root through traversal, a symbolic link, or a
    /// junction.
    EscapesRoot(String),
    /// A create/replace target is an existing symbolic link or junction.
    SymlinkLeaf(WorkspacePath),
    /// The location is AiDEX-managed or otherwise must not become (or be deleted as)
    /// workspace data.
    Forbidden(String),
    /// The operation needs a different workspace kind (e.g. ephemeral-only).
    WrongKind {
        workspace: String,
        expected: &'static str,
    },
    /// An apply selection names a path that is not a reviewable change.
    NotAChange(WorkspacePath),
    /// The ephemeral workspace changed after the review the caller approved.
    ReviewChanged { expected: String, actual: String },
    /// A copy exceeded the configured file-count or byte limit; nothing was kept.
    TooLarge(String),
    /// The AiDEX ownership marker is missing or does not match; nothing was deleted.
    OwnershipMarker(PathBuf),
    /// The registry database was written by a newer AiDEX build.
    Incompatible(String),
    /// A drive directory could not be resolved or created safely.
    Drive(DriveError),
    /// The registry database failed.
    Storage(String),
    /// Persisted AiDEX metadata (marker, baseline, registry row) is unreadable.
    Corrupt(String),
    /// An operating-system I/O failure, with the operation that failed.
    Io { context: String, source: io::Error },
}

impl WorkspaceError {
    /// Stable service category for API mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::InvalidPath { .. }
            | Self::InvalidName(_)
            | Self::WrongKind { .. }
            | Self::NotAChange(_)
            | Self::TooLarge(_) => ServiceErrorCode::InvalidArgument,
            Self::NotFound(_) | Self::PathNotFound(_) | Self::RootUnavailable { .. } => {
                ServiceErrorCode::NotFound
            }
            Self::Conflict(_) | Self::ReviewChanged { .. } => ServiceErrorCode::Conflict,
            Self::EscapesRoot(_)
            | Self::SymlinkLeaf(_)
            | Self::Forbidden(_)
            | Self::OwnershipMarker(_) => ServiceErrorCode::PermissionDenied,
            Self::Incompatible(_) => ServiceErrorCode::IncompatibleVersion,
            Self::Drive(error) => error.code(),
            Self::Storage(_) | Self::Corrupt(_) | Self::Io { .. } => ServiceErrorCode::Internal,
        }
    }

    /// Stable snake_case reason, finer-grained than [`Self::code`], for `data.reason`.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::InvalidPath { .. } => "invalid_path",
            Self::InvalidName(_) => "invalid_name",
            Self::NotFound(_) => "workspace_not_found",
            Self::PathNotFound(_) => "path_not_found",
            Self::RootUnavailable { .. } => "root_unavailable",
            Self::Conflict(_) => "conflict",
            Self::EscapesRoot(_) => "escapes_root",
            Self::SymlinkLeaf(_) => "symlink_leaf",
            Self::Forbidden(_) => "forbidden_location",
            Self::WrongKind { .. } => "wrong_kind",
            Self::NotAChange(_) => "not_a_change",
            Self::ReviewChanged { .. } => "review_changed",
            Self::TooLarge(_) => "too_large",
            Self::OwnershipMarker(_) => "ownership_marker",
            Self::Incompatible(_) => "incompatible_registry",
            Self::Drive(_) => "drive",
            Self::Storage(_) => "storage",
            Self::Corrupt(_) => "corrupt_metadata",
            Self::Io { .. } => "io",
        }
    }

    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    pub(crate) fn invalid_path(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidPath {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath { path, reason } => write!(f, "invalid path `{path}`: {reason}"),
            Self::InvalidName(reason) => write!(f, "invalid workspace name: {reason}"),
            Self::NotFound(reference) => write!(f, "no workspace `{reference}` is registered"),
            Self::PathNotFound(path) => {
                write!(f, "`{}` does not exist in the workspace", path.as_str())
            }
            Self::RootUnavailable { workspace, reason } => {
                write!(f, "workspace `{workspace}` is unavailable: {reason}")
            }
            Self::Conflict(message) | Self::TooLarge(message) => f.write_str(message),
            Self::EscapesRoot(path) => write!(
                f,
                "`{path}` resolves outside the workspace root (traversal, symbolic link, or junction)"
            ),
            Self::SymlinkLeaf(path) => write!(
                f,
                "refusing to replace `{}`: it is a symbolic link or junction",
                path.as_str()
            ),
            Self::Forbidden(message) => f.write_str(message),
            Self::WrongKind {
                workspace,
                expected,
            } => write!(f, "workspace `{workspace}` is not {expected}"),
            Self::NotAChange(path) => write!(
                f,
                "`{}` is not a reviewable change in this ephemeral workspace",
                path.as_str()
            ),
            Self::ReviewChanged { expected, actual } => write!(
                f,
                "the ephemeral workspace changed after review (reviewed {expected}, now {actual}); review it again"
            ),
            Self::OwnershipMarker(path) => write!(
                f,
                "refusing to delete `{}`: its AiDEX ownership marker is missing or does not match",
                path.display()
            ),
            Self::Incompatible(message) => f.write_str(message),
            Self::Drive(error) => error.fmt(f),
            Self::Storage(message) => write!(f, "workspace registry error: {message}"),
            Self::Corrupt(message) => write!(f, "workspace metadata is corrupt: {message}"),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
        }
    }
}

impl Error for WorkspaceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Drive(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<DriveError> for WorkspaceError {
    fn from(error: DriveError) -> Self {
        Self::Drive(error)
    }
}

impl From<rusqlite::Error> for WorkspaceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}
