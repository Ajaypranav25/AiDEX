//! Stable errors for the context service.

use std::{fmt, io};

use aidex_core::{EntityId, ServiceErrorCode};
use aidex_workspace::WorkspaceError;

use crate::memory::MemoryError;

/// Everything that can go wrong while indexing, searching, loading project rules,
/// or assembling a context pack.
#[derive(Debug)]
pub enum ContextError {
    /// A search query was empty, too long, or otherwise unusable.
    InvalidQuery(String),
    /// A request (pack, index options, identifiers) was malformed.
    InvalidRequest(String),
    /// Path confinement or workspace lookup failed.
    Workspace(WorkspaceError),
    /// The drive refused to create or open a managed directory.
    Drive(String),
    /// An indexing job is already running for this workspace.
    IndexBusy(EntityId),
    /// The index database failed (not corruption, which is recovered on open).
    Index(String),
    /// `.aidex/rules.toml` exists but is invalid, unsafe, or unreadable.
    Rules(String),
    /// Memory store failure.
    Memory(MemoryError),
    /// Work stopped because cancellation was requested.
    Cancelled,
    /// Filesystem failure.
    Io { context: String, source: io::Error },
}

impl ContextError {
    /// The transport-safe code for API mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::InvalidQuery(_) | Self::InvalidRequest(_) | Self::Rules(_) => {
                ServiceErrorCode::InvalidArgument
            }
            Self::Workspace(error) => error.code(),
            Self::Memory(error) => error.code(),
            Self::IndexBusy(_) => ServiceErrorCode::Conflict,
            Self::Cancelled => ServiceErrorCode::Unavailable,
            Self::Drive(_) | Self::Index(_) | Self::Io { .. } => ServiceErrorCode::Internal,
        }
    }

    /// A finer stable reason for `data.reason`.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::InvalidQuery(_) => "invalid_query",
            Self::InvalidRequest(_) => "invalid_request",
            Self::Workspace(error) => error.reason(),
            Self::Drive(_) => "drive",
            Self::IndexBusy(_) => "index_busy",
            Self::Index(_) => "index",
            Self::Rules(_) => "invalid_rules",
            Self::Memory(error) => error.reason(),
            Self::Cancelled => "cancelled",
            Self::Io { .. } => "io",
        }
    }

    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    pub(crate) fn sqlite(context: &str, error: rusqlite::Error) -> Self {
        Self::Index(format!("{context}: {error}"))
    }
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery(message) => write!(f, "invalid search query: {message}"),
            Self::InvalidRequest(message) => write!(f, "invalid context request: {message}"),
            Self::Workspace(error) => error.fmt(f),
            Self::Drive(message) => write!(f, "drive error: {message}"),
            Self::IndexBusy(workspace) => {
                write!(f, "workspace `{workspace}` is already being indexed")
            }
            Self::Index(message) => write!(f, "index error: {message}"),
            Self::Rules(message) => write!(f, "invalid project rules: {message}"),
            Self::Memory(error) => error.fmt(f),
            Self::Cancelled => f.write_str("the operation was cancelled"),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
        }
    }
}

impl std::error::Error for ContextError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Workspace(error) => Some(error),
            Self::Memory(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<WorkspaceError> for ContextError {
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}

impl From<MemoryError> for ContextError {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error)
    }
}

impl From<aidex_drive::DriveError> for ContextError {
    fn from(error: aidex_drive::DriveError) -> Self {
        Self::Drive(error.to_string())
    }
}

/// Reject identifiers that cannot safely name a file on every supported OS.
pub(crate) fn file_safe_id(id: &EntityId) -> Result<&str, ContextError> {
    let text = id.as_str();
    let safe = !text.starts_with('.')
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if safe {
        Ok(text)
    } else {
        Err(ContextError::InvalidRequest(format!(
            "identifier `{text}` cannot name an index or memory file"
        )))
    }
}
