//! Event-log errors with stable service categories.

use std::{fmt, path::PathBuf};

use aidex_core::ServiceErrorCode;

use crate::Cursor;

/// Every failure of the event contract. Messages never contain event payload text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventError {
    /// SQLite or filesystem failure; nothing was assigned or changed.
    Storage(String),
    /// Retention removed history after `requested`; resume no earlier than
    /// `oldest_available - 1` (an explicit, acknowledged gap) or start over.
    CursorExpired {
        requested: Cursor,
        oldest_available: Cursor,
    },
    /// The cursor is newer than anything this log ever assigned (wrong drive or a
    /// replaced database); resuming from it would silently skip events.
    CursorAhead {
        requested: Cursor,
        latest: Cursor,
    },
    /// Cursors are limited to the SQLite integer range (`0..=i64::MAX`).
    InvalidCursor,
    /// A page size or subscription bound outside `1..=max`.
    InvalidLimit {
        requested: usize,
        max: usize,
    },
    InvalidRetention(&'static str),
    InvalidFilter(String),
    InvalidScope(&'static str),
    /// An identifier (scope or payload) looks like a credential; the event was refused.
    SensitiveIdentifier,
    /// The encoded event exceeds [`crate::MAX_EVENT_BYTES`].
    TooLarge {
        bytes: usize,
        limit: usize,
    },
    /// The envelope was written by an unsupported contract version.
    UnsupportedVersion {
        found: u64,
    },
    /// A stored or received envelope is malformed.
    Decode(String),
    /// The database schema is newer (or older) than this build understands.
    IncompatibleSchema {
        found: i64,
        supported: i64,
    },
    /// Importing a PR #40 (v1) event log failed; nothing was imported.
    Migration(String),
    /// A managed database path is a symbolic link.
    UnsafePath(PathBuf),
}

impl EventError {
    /// Stable category for API and CLI mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Storage(_) | Self::Decode(_) | Self::Migration(_) => ServiceErrorCode::Internal,
            Self::CursorExpired { .. } => ServiceErrorCode::NotFound,
            Self::CursorAhead { .. }
            | Self::InvalidCursor
            | Self::InvalidLimit { .. }
            | Self::InvalidRetention(_)
            | Self::InvalidFilter(_)
            | Self::InvalidScope(_)
            | Self::SensitiveIdentifier
            | Self::TooLarge { .. } => ServiceErrorCode::InvalidArgument,
            Self::UnsupportedVersion { .. } | Self::IncompatibleSchema { .. } => {
                ServiceErrorCode::IncompatibleVersion
            }
            Self::UnsafePath(_) => ServiceErrorCode::PermissionDenied,
        }
    }
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(message) => write!(f, "event storage error: {message}"),
            Self::CursorExpired {
                requested,
                oldest_available,
            } => write!(
                f,
                "event cursor {requested} expired; the oldest retained event is {oldest_available}"
            ),
            Self::CursorAhead { requested, latest } => write!(
                f,
                "event cursor {requested} is ahead of this log (latest is {latest}); it belongs to another drive or a replaced log"
            ),
            Self::InvalidCursor => write!(f, "event cursor exceeds the supported range"),
            Self::InvalidLimit { requested, max } => {
                write!(f, "limit {requested} must be between 1 and {max}")
            }
            Self::InvalidRetention(reason) => write!(f, "invalid event retention: {reason}"),
            Self::InvalidFilter(reason) => write!(f, "invalid event filter: {reason}"),
            Self::InvalidScope(reason) => write!(f, "invalid event scope: {reason}"),
            Self::SensitiveIdentifier => write!(
                f,
                "an event identifier looks like a credential; the event was refused"
            ),
            Self::TooLarge { bytes, limit } => {
                write!(
                    f,
                    "encoded event of {bytes} bytes exceeds the {limit}-byte limit"
                )
            }
            Self::UnsupportedVersion { found } => {
                write!(f, "unsupported event envelope version {found}")
            }
            Self::Decode(reason) => write!(f, "invalid event envelope: {reason}"),
            Self::IncompatibleSchema { found, supported } => write!(
                f,
                "event database schema version {found} is not supported (this build supports {supported})"
            ),
            Self::Migration(reason) => write!(f, "event log migration failed: {reason}"),
            Self::UnsafePath(path) => {
                write!(
                    f,
                    "managed event database path is a symlink: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for EventError {}

pub(crate) fn storage(error: impl fmt::Display) -> EventError {
    EventError::Storage(error.to_string())
}
