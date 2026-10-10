//! Durable, versioned workspace event log with cursor-based replay.
//!
//! Events contain only typed, non-secret state transitions. Consumers acknowledge a cursor
//! only after processing; after disconnect they call [`EventLog::read_after`] with that cursor.
//! Retention is explicit: a cursor older than retained history returns [`EventError::CursorExpired`].
//! Live subscriptions are bounded and best-effort; a slow consumer is detached and must replay.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, Weak,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

pub const EVENT_VERSION: u32 = 1;
pub const MAX_BATCH: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub version: u32,
    /// Monotonic within one workspace log; this is the replay cursor.
    pub sequence: u64,
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub kind: EventKind,
}

/// Typed product transitions. No variant accepts arbitrary payloads or credentials.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventKind {
    TaskStarted {
        task_id: String,
    },
    TaskFinished {
        task_id: String,
        outcome: TransitionOutcome,
    },
    SessionStarted,
    SessionFinished,
    AgentStarted {
        agent_id: String,
    },
    AgentFinished {
        agent_id: String,
        outcome: TransitionOutcome,
    },
    ToolStarted {
        tool_id: String,
    },
    ToolFinished {
        tool_id: String,
        outcome: TransitionOutcome,
    },
    ApprovalRequested {
        approval_id: String,
    },
    ApprovalResolved {
        approval_id: String,
        decision: ApprovalDecision,
    },
    ProcessStarted {
        process_id: String,
    },
    ProcessFinished {
        process_id: String,
        exit_code: Option<i32>,
    },
    ModelStarted {
        model_id: String,
    },
    ModelFinished {
        model_id: String,
        outcome: TransitionOutcome,
    },
    CleanupStarted {
        cleanup_id: String,
    },
    CleanupFinished {
        cleanup_id: String,
        outcome: TransitionOutcome,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Denied,
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventError {
    Storage(String),
    CursorExpired { requested: u64, earliest: u64 },
    InvalidLimit,
    InvalidCursor,
    InvalidIdentifier,
}
impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(message) => write!(f, "event storage error: {message}"),
            Self::CursorExpired {
                requested,
                earliest,
            } => write!(
                f,
                "event cursor {requested} expired; earliest available sequence is {earliest}"
            ),
            Self::InvalidLimit => write!(
                f,
                "event read limit and subscription capacity must be between 1 and 256"
            ),
            Self::InvalidCursor => write!(f, "event cursor exceeds the supported range"),
            Self::InvalidIdentifier => write!(
                f,
                "event identifiers may contain only ASCII letters, digits, dot, underscore, colon, or hyphen and must be at most 128 bytes"
            ),
        }
    }
}
impl std::error::Error for EventError {}

struct Subscriber {
    id: u64,
    workspace_id: String,
    sender: SyncSender<Event>,
}
struct Inner {
    connection: Connection,
    subscribers: Vec<Subscriber>,
    next_subscriber_id: u64,
}

/// SQLite-backed event store. Retention is per workspace and bounded by event count.
pub struct EventLog {
    path: PathBuf,
    retention: u64,
    inner: Arc<Mutex<Inner>>,
}

/// Bounded, best-effort live feed. A disconnected receiver reconnects with its last
/// acknowledged event sequence using [`EventLog::read_after`].
pub struct Subscription {
    receiver: Receiver<Event>,
    inner: Weak<Mutex<Inner>>,
    id: u64,
}
impl Subscription {
    pub fn recv(&self) -> Result<Event, mpsc::RecvError> {
        self.receiver.recv()
    }
    pub fn try_recv(&self) -> Result<Event, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Event, mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .subscribers
                .retain(|subscriber| subscriber.id != self.id);
        }
    }
}

impl EventLog {
    /// Open or create a persistent event database. `retention` must be positive.
    pub fn open(path: impl AsRef<Path>, retention: u64) -> Result<Self, EventError> {
        if retention == 0 {
            return Err(EventError::InvalidLimit);
        }
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(storage_error)?;
        }
        let connection = Connection::open(&path).map_err(storage_error)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS events (
                 workspace_id TEXT NOT NULL,
                 sequence INTEGER NOT NULL,
                 session_id TEXT,
                 body TEXT NOT NULL,
                 PRIMARY KEY(workspace_id, sequence)
             );",
            )
            .map_err(storage_error)?;
        Ok(Self {
            path,
            retention,
            inner: Arc::new(Mutex::new(Inner {
                connection,
                subscribers: Vec::new(),
                next_subscriber_id: 1,
            })),
        })
    }

    /// Append and durably commit before returning. Retention removes only the oldest
    /// rows after commit; consumers outside retention receive an explicit cursor error.
    pub fn publish(
        &self,
        workspace_id: impl Into<String>,
        session_id: Option<String>,
        kind: EventKind,
    ) -> Result<Event, EventError> {
        let workspace_id = workspace_id.into();
        validate_identifier(&workspace_id)?;
        if session_id
            .as_deref()
            .is_some_and(|id| !valid_identifier(id))
            || !valid_kind_identifiers(&kind)
        {
            return Err(EventError::InvalidIdentifier);
        }
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let tx = inner.connection.transaction().map_err(storage_error)?;
        let previous: Option<i64> = tx
            .query_row(
                "SELECT MAX(sequence) FROM events WHERE workspace_id = ?1",
                [&workspace_id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let sequence = previous
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| EventError::Storage("sequence exhausted".into()))?
            as u64;
        let event = Event {
            version: EVENT_VERSION,
            sequence,
            workspace_id: workspace_id.clone(),
            session_id: session_id.clone(),
            kind,
        };
        let body = serde_json::to_string(&event).map_err(storage_error)?;
        tx.execute(
            "INSERT INTO events(workspace_id, sequence, session_id, body) VALUES (?1, ?2, ?3, ?4)",
            params![workspace_id, sequence as i64, session_id, body],
        )
        .map_err(storage_error)?;
        let cutoff = sequence.saturating_sub(self.retention);
        tx.execute(
            "DELETE FROM events WHERE workspace_id = ?1 AND sequence <= ?2",
            params![workspace_id, cutoff as i64],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        inner.subscribers.retain(|subscriber| {
            if subscriber.workspace_id != event.workspace_id {
                return true;
            }
            match subscriber.sender.try_send(event.clone()) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
            }
        });
        Ok(event)
    }

    /// Replay events in sequence order after an acknowledged workspace cursor.
    pub fn read_after(
        &self,
        workspace_id: &str,
        cursor: u64,
        limit: usize,
    ) -> Result<Vec<Event>, EventError> {
        if limit == 0 || limit > MAX_BATCH {
            return Err(EventError::InvalidLimit);
        }
        if cursor > i64::MAX as u64 {
            return Err(EventError::InvalidCursor);
        }
        validate_identifier(workspace_id)?;
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let earliest: Option<i64> = inner
            .connection
            .query_row(
                "SELECT MIN(sequence) FROM events WHERE workspace_id = ?1",
                [workspace_id],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if let Some(earliest) = earliest {
            if cursor.saturating_add(1) < earliest as u64 {
                return Err(EventError::CursorExpired {
                    requested: cursor,
                    earliest: earliest as u64,
                });
            }
        }
        let mut statement = inner.connection.prepare(
            "SELECT body FROM events WHERE workspace_id = ?1 AND sequence > ?2 ORDER BY sequence LIMIT ?3"
        ).map_err(storage_error)?;
        let rows = statement
            .query_map(params![workspace_id, cursor as i64, limit as i64], |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage_error)?;
        rows.map(|row| serde_json::from_str(&row.map_err(storage_error)?).map_err(storage_error))
            .collect()
    }

    /// Start a bounded live subscription. `capacity` limits queued events; overflow
    /// detaches the receiver rather than blocking a publisher or buffering without bound.
    pub fn subscribe(
        &self,
        workspace_id: impl Into<String>,
        capacity: usize,
    ) -> Result<Subscription, EventError> {
        if capacity == 0 || capacity > MAX_BATCH {
            return Err(EventError::InvalidLimit);
        }
        let workspace_id = workspace_id.into();
        validate_identifier(&workspace_id)?;
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let id = inner.next_subscriber_id;
        inner.next_subscriber_id = id
            .checked_add(1)
            .ok_or_else(|| EventError::Storage("subscription identifiers exhausted".into()))?;
        inner.subscribers.push(Subscriber {
            id,
            workspace_id,
            sender,
        });
        Ok(Subscription {
            receiver,
            inner: Arc::downgrade(&self.inner),
            id,
        })
    }

    pub fn database_path(&self) -> &Path {
        &self.path
    }
}
fn validate_identifier(identifier: &str) -> Result<(), EventError> {
    valid_identifier(identifier)
        .then_some(())
        .ok_or(EventError::InvalidIdentifier)
}

fn valid_identifier(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier.len() <= 128
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn valid_kind_identifiers(kind: &EventKind) -> bool {
    match kind {
        EventKind::TaskStarted { task_id } | EventKind::TaskFinished { task_id, .. } => {
            valid_identifier(task_id)
        }
        EventKind::AgentStarted { agent_id } | EventKind::AgentFinished { agent_id, .. } => {
            valid_identifier(agent_id)
        }
        EventKind::ToolStarted { tool_id } | EventKind::ToolFinished { tool_id, .. } => {
            valid_identifier(tool_id)
        }
        EventKind::ApprovalRequested { approval_id }
        | EventKind::ApprovalResolved { approval_id, .. } => valid_identifier(approval_id),
        EventKind::ProcessStarted { process_id }
        | EventKind::ProcessFinished { process_id, .. } => valid_identifier(process_id),
        EventKind::ModelStarted { model_id } | EventKind::ModelFinished { model_id, .. } => {
            valid_identifier(model_id)
        }
        EventKind::CleanupStarted { cleanup_id }
        | EventKind::CleanupFinished { cleanup_id, .. } => valid_identifier(cleanup_id),
        EventKind::SessionStarted | EventKind::SessionFinished => true,
    }
}

fn storage_error(error: impl std::fmt::Display) -> EventError {
    EventError::Storage(error.to_string())
}
