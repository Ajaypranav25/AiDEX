//! Session-store errors with stable service categories and reasons.

use std::{fmt, path::PathBuf};

use aidex_core::ServiceErrorCode;
use aidex_storage::StoreError;

use crate::SessionStateKind;

/// Every failure of the session store. Messages never contain message, tool, or
/// checkpoint payload text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionError {
    /// No session, checkpoint, artifact, tool call, or approval with this id.
    NotFound { kind: &'static str, id: String },
    /// The state machine does not allow `from → to`.
    InvalidTransition {
        from: SessionStateKind,
        to: SessionStateKind,
    },
    /// The session is completed, failed, or cancelled; it accepts no new records.
    SessionClosed { id: String, state: SessionStateKind },
    /// A caller-supplied value is malformed or too large.
    InvalidArgument(String),
    /// A record with this id already exists, or the request contradicts stored state.
    Conflict(String),
    /// The checkpoint blob failed hash or envelope verification and was marked corrupt.
    CheckpointCorrupt {
        checkpoint_id: String,
        reason: String,
    },
    /// Every checkpoint of the session failed verification; nothing can be restored.
    NoValidCheckpoint { session_id: String, corrupt: usize },
    /// The database or a checkpoint was written by a newer (or unknown) format.
    Incompatible(String),
    /// A stored row is malformed.
    Corrupt(String),
    /// A managed database or salt path is a symbolic link (or not a regular file).
    UnsafePath(PathBuf),
    /// The drive is missing, full, or read-only; nothing was changed.
    Unavailable(String),
    /// SQLite or filesystem failure.
    Storage(String),
}

impl SessionError {
    pub(crate) fn not_found(kind: &'static str, id: impl fmt::Display) -> Self {
        Self::NotFound {
            kind,
            id: id.to_string(),
        }
    }

    /// Stable category for API and CLI mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::NotFound { .. } => ServiceErrorCode::NotFound,
            Self::InvalidTransition { .. } | Self::SessionClosed { .. } | Self::Conflict(_) => {
                ServiceErrorCode::Conflict
            }
            Self::InvalidArgument(_) => ServiceErrorCode::InvalidArgument,
            Self::CheckpointCorrupt { .. }
            | Self::NoValidCheckpoint { .. }
            | Self::Corrupt(_)
            | Self::Storage(_) => ServiceErrorCode::Internal,
            Self::Incompatible(_) => ServiceErrorCode::IncompatibleVersion,
            Self::UnsafePath(_) => ServiceErrorCode::PermissionDenied,
            Self::Unavailable(_) => ServiceErrorCode::Unavailable,
        }
    }

    /// Stable snake_case reason for `data.reason` in daemon errors.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::NotFound { kind, .. } => match *kind {
                "session" => "session_not_found",
                "checkpoint" => "checkpoint_not_found",
                "artifact" => "artifact_not_found",
                "tool call" => "tool_call_not_found",
                "approval" => "approval_not_found",
                _ => "not_found",
            },
            Self::InvalidTransition { .. } => "invalid_transition",
            Self::SessionClosed { .. } => "session_closed",
            Self::InvalidArgument(_) => "invalid_argument",
            Self::Conflict(_) => "conflict",
            Self::CheckpointCorrupt { .. } => "checkpoint_corrupt",
            Self::NoValidCheckpoint { .. } => "no_valid_checkpoint",
            Self::Incompatible(_) => "incompatible",
            Self::Corrupt(_) => "corrupt",
            Self::UnsafePath(_) => "unsafe_path",
            Self::Unavailable(_) => "unavailable",
            Self::Storage(_) => "storage",
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { kind, id } => write!(f, "{kind} `{id}` was not found"),
            Self::InvalidTransition { from, to } => write!(
                f,
                "a session cannot move from `{}` to `{}`",
                from.as_str(),
                to.as_str()
            ),
            Self::SessionClosed { id, state } => write!(
                f,
                "session `{id}` is {} and accepts no new records",
                state.as_str()
            ),
            Self::InvalidArgument(message) => write!(f, "invalid session request: {message}"),
            Self::Conflict(message) => write!(f, "session conflict: {message}"),
            Self::CheckpointCorrupt {
                checkpoint_id,
                reason,
            } => write!(f, "checkpoint `{checkpoint_id}` is corrupt: {reason}"),
            Self::NoValidCheckpoint {
                session_id,
                corrupt,
            } => write!(
                f,
                "session `{session_id}` has no valid checkpoint ({corrupt} failed verification)"
            ),
            Self::Incompatible(message) => write!(f, "incompatible session data: {message}"),
            Self::Corrupt(message) => write!(f, "session database is corrupt: {message}"),
            Self::UnsafePath(path) => write!(
                f,
                "refusing to use `{}`: it is a symbolic link or not a regular file",
                path.display()
            ),
            Self::Unavailable(message) => write!(f, "session storage is unavailable: {message}"),
            Self::Storage(message) => write!(f, "session storage error: {message}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<rusqlite::Error> for SessionError {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(failure, _) = &error {
            use rusqlite::ErrorCode;
            match failure.code {
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => {
                    return Self::Corrupt(error.to_string());
                }
                ErrorCode::DiskFull
                | ErrorCode::ReadOnly
                | ErrorCode::CannotOpen
                | ErrorCode::NotFound => return Self::Unavailable(error.to_string()),
                _ => {}
            }
        }
        Self::Storage(error.to_string())
    }
}

impl From<StoreError> for SessionError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Corrupt(message) => Self::Corrupt(message),
            StoreError::LowSpace(message)
            | StoreError::ReadOnly(message)
            | StoreError::DriveUnavailable(message) => Self::Unavailable(message),
            StoreError::Migration(message) => Self::Incompatible(message),
            other => Self::Storage(other.to_string()),
        }
    }
}

impl From<std::io::Error> for SessionError {
    fn from(error: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match error.kind() {
            ErrorKind::NotFound | ErrorKind::StorageFull | ErrorKind::ReadOnlyFilesystem => {
                Self::Unavailable(error.to_string())
            }
            _ => Self::Storage(error.to_string()),
        }
    }
}

impl From<aidex_drive::DriveError> for SessionError {
    fn from(error: aidex_drive::DriveError) -> Self {
        match error {
            aidex_drive::DriveError::UnsafeComponent(path) => Self::UnsafePath(path),
            error => match error.code() {
                ServiceErrorCode::IncompatibleVersion => Self::Incompatible(error.to_string()),
                ServiceErrorCode::Unavailable | ServiceErrorCode::PermissionDenied => {
                    Self::Unavailable(error.to_string())
                }
                _ => Self::Storage(error.to_string()),
            },
        }
    }
}
