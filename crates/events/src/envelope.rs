//! The versioned event envelope, its global cursor, and its scope.

use std::fmt;

use aidex_core::{ContractVersion, EntityId, Timestamp};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;

use crate::{EventError, EventKind};

/// Envelope contract version. Version 1 was PR #40's per-workspace `Event` record.
pub const ENVELOPE_VERSION: u16 = 2;

/// Position in the drive's single, strictly increasing event log.
///
/// Durable events get consecutive cursors starting at 1; `Cursor::ZERO` means
/// "before everything". Consumers acknowledge the cursor of the last event they
/// processed and resume strictly after it.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Cursor(u64);

impl Cursor {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The SQLite representation; cursors above `i64::MAX` are not representable.
    pub(crate) fn to_sql(self) -> Result<i64, EventError> {
        i64::try_from(self.0).map_err(|_| EventError::InvalidCursor)
    }

    pub(crate) fn from_sql(value: i64) -> Result<Self, EventError> {
        u64::try_from(value)
            .map(Self)
            .map_err(|_| EventError::Storage(format!("negative cursor {value} in event log")))
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Who an event belongs to. A session scope always names its workspace.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<EntityId>,
}

impl EventScope {
    /// Drive-wide events (daemon, vault, drive, remote, ...).
    pub fn global() -> Self {
        Self::default()
    }

    pub fn workspace(workspace_id: EntityId) -> Self {
        Self {
            workspace_id: Some(workspace_id),
            session_id: None,
        }
    }

    pub fn session(workspace_id: EntityId, session_id: EntityId) -> Self {
        Self {
            workspace_id: Some(workspace_id),
            session_id: Some(session_id),
        }
    }

    /// Reject a session without its workspace.
    pub fn validate(&self) -> Result<(), EventError> {
        if self.session_id.is_some() && self.workspace_id.is_none() {
            return Err(EventError::InvalidScope(
                "a session-scoped event must also name its workspace",
            ));
        }
        Ok(())
    }
}

/// One event as stored, replayed, and streamed.
///
/// Wire form (`version` is checked before anything else is decoded):
/// `{"version":2,"cursor":7,"scope":{"workspace_id":"..."},"occurred_at":"...Z","event":{"type":"...","data":{...}}}`.
///
/// Ephemeral events (`event.is_durable() == false`) are never stored; their cursor
/// is that of the latest durable event when they were emitted, so they sort after it.
/// Consumers must not acknowledge or deduplicate by an ephemeral event's cursor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EventEnvelope {
    pub version: ContractVersion,
    pub cursor: Cursor,
    pub scope: EventScope,
    /// Wall-clock time of the append. Ordering is defined by `cursor`, not by time.
    pub occurred_at: Timestamp,
    pub event: EventKind,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeFields {
    version: ContractVersion,
    cursor: Cursor,
    scope: EventScope,
    occurred_at: Timestamp,
    event: EventKind,
}

impl EventEnvelope {
    /// The envelope version this build writes and accepts.
    pub fn current_version() -> ContractVersion {
        // `ENVELOPE_VERSION` is a non-zero constant, so this never falls back.
        ContractVersion::new(ENVELOPE_VERSION).unwrap_or(ContractVersion::CURRENT)
    }

    pub(crate) fn new(
        cursor: Cursor,
        scope: EventScope,
        occurred_at: Timestamp,
        event: EventKind,
    ) -> Self {
        Self {
            version: Self::current_version(),
            cursor,
            scope,
            occurred_at,
            event,
        }
    }

    /// Shorthand for `self.event.name()`.
    pub fn name(&self) -> &'static str {
        self.event.name()
    }

    pub fn is_durable(&self) -> bool {
        self.event.is_durable()
    }

    /// Decode JSON, rejecting unknown envelope versions before reading the payload.
    pub fn decode(json: &str) -> Result<Self, EventError> {
        let value: Value =
            serde_json::from_str(json).map_err(|error| EventError::Decode(error.to_string()))?;
        Self::from_value(value)
    }

    fn from_value(value: Value) -> Result<Self, EventError> {
        match value.get("version").and_then(Value::as_u64) {
            Some(version) if version == u64::from(ENVELOPE_VERSION) => {}
            Some(found) => return Err(EventError::UnsupportedVersion { found }),
            None => {
                return Err(EventError::Decode(
                    "missing or invalid envelope version".into(),
                ));
            }
        }
        let fields: EnvelopeFields =
            serde_json::from_value(value).map_err(|error| EventError::Decode(error.to_string()))?;
        Ok(Self {
            version: fields.version,
            cursor: fields.cursor,
            scope: fields.scope,
            occurred_at: fields.occurred_at,
            event: fields.event,
        })
    }
}

impl<'de> Deserialize<'de> for EventEnvelope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_value(Value::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}
