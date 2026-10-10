//! Logical checkpoints (spec §21) and branching from them (spec §74).
//!
//! A checkpoint is a versioned JSON envelope — the harness-owned [`LogicalState`]
//! plus store-captured [`HighWaterMarks`] — stored as a content-addressed blob whose
//! SHA-256 and size are recorded in SQLite. Restoring verifies the hash, size,
//! envelope version, and that the envelope names the checkpoint and session it is
//! recorded for. A checkpoint that fails is marked `corrupt` (never deleted) and the
//! newest older valid checkpoint is used instead, flagged
//! `recovered_from_corruption`. Host processes are never part of a checkpoint.

use aidex_core::{EntityId, ModelReference, Timestamp};
use aidex_events::{Cursor, EventKind};
use aidex_storage::StoreError;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    BudgetUsage, CheckpointId, HostFingerprint, Session, SessionError, SessionState,
    SessionStateKind, SessionStore, ToolCallState,
    approvals::load_approvals,
    model::{MAX_TITLE_BYTES, bounded},
    secrets::{redact, redact_json},
    store::{
        insert_session,
        publish::{Pending, stage},
        rows::{self, load_open_session, load_session, signed},
    },
    tool_calls::load_tool_calls,
};

/// Envelope version written by this build.
pub const CHECKPOINT_FORMAT: u32 = 1;
/// Largest encoded checkpoint, in bytes.
pub const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
/// Longest checkpoint label, in bytes.
pub const MAX_LABEL_BYTES: usize = 256;

/// What the harness needs to reconstruct its logical state. Opaque to the store,
/// except that every string in it is redacted before it is written.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogicalState {
    /// Harness-owned payload (plan position, compacted context references, ...).
    pub harness: Value,
    /// The agent task graph (spec §8.2), if the harness keeps one.
    #[serde(default)]
    pub task_graph: Option<Value>,
}

impl LogicalState {
    pub fn new(harness: Value) -> Self {
        Self {
            harness,
            task_graph: None,
        }
    }

    #[must_use]
    pub fn with_task_graph(mut self, graph: Value) -> Self {
        self.task_graph = Some(graph);
        self
    }
}

/// Durable positions captured by the store when the checkpoint was written.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HighWaterMarks {
    /// Last message included (`0` = none).
    pub message_seq: u64,
    /// Every tool call recorded so far and its state at checkpoint time.
    pub tool_calls: Vec<ToolCallMark>,
    pub event_cursor: Cursor,
    pub model: Option<ModelReference>,
    /// Pending, unexpired approvals; they are never restored automatically.
    pub open_approvals: Vec<EntityId>,
    pub budget: BudgetUsage,
    pub session_state: SessionStateKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallMark {
    pub call_id: EntityId,
    pub tool_id: EntityId,
    pub state: ToolCallState,
}

/// Verification status recorded for a checkpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    /// Not (yet) found to be damaged.
    Valid,
    /// Failed verification; kept for inspection, never restored.
    Corrupt,
}

/// A checkpoint's SQLite record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointMeta {
    /// `chk_<ULID>`.
    pub id: CheckpointId,
    pub session_id: EntityId,
    /// 1-based, strictly increasing per session.
    pub seq: u64,
    pub label: Option<String>,
    /// SHA-256 of the envelope blob in the session `FileStore`.
    pub sha256: String,
    pub size: u64,
    pub format: u32,
    pub message_seq: u64,
    pub created_at: Timestamp,
    pub status: CheckpointStatus,
    pub corrupt_reason: Option<String>,
}

/// The stored envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: u32,
    checkpoint_id: CheckpointId,
    session_id: EntityId,
    workspace_id: EntityId,
    created_at: Timestamp,
    label: Option<String>,
    host: HostFingerprint,
    seeded_from: Option<CheckpointId>,
    marks: HighWaterMarks,
    state: LogicalState,
}

/// A verified, decoded checkpoint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub meta: CheckpointMeta,
    /// The host the checkpoint was written on.
    pub host_fingerprint: HostFingerprint,
    /// For a branch's seed checkpoint: the parent checkpoint it copies.
    pub seeded_from: Option<CheckpointId>,
    pub marks: HighWaterMarks,
    pub state: LogicalState,
}

/// Result of [`SessionStore::latest_valid_checkpoint`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoredCheckpoint {
    pub checkpoint: Checkpoint,
    /// A newer checkpoint exists but is corrupt; this older one was used instead.
    pub recovered_from_corruption: bool,
    /// Newer checkpoints skipped because they are corrupt, newest first.
    pub corrupt_checkpoints: Vec<CheckpointId>,
}

const CHECKPOINT_COLUMNS: &str = "id, session_id, seq, label, sha256, size, format, message_seq, \
    created_at, status, corrupt_reason";

type CheckpointRow = (
    String,
    String,
    i64,
    Option<String>,
    String,
    i64,
    i64,
    i64,
    String,
    String,
    Option<String>,
);

fn checkpoint_row(row: &Row<'_>) -> rusqlite::Result<CheckpointRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
    ))
}

fn decode_meta(
    (id, session, seq, label, sha256, size, format, message_seq, created, status, reason): CheckpointRow,
) -> Result<CheckpointMeta, SessionError> {
    Ok(CheckpointMeta {
        id: rows::id(id, "checkpoint id")?,
        session_id: rows::id(session, "checkpoint session id")?,
        seq: rows::unsigned(seq, "checkpoint seq")?,
        label,
        sha256,
        size: rows::unsigned(size, "checkpoint size")?,
        format: u32::try_from(format).map_err(|_| rows::corrupt("invalid checkpoint format"))?,
        message_seq: rows::unsigned(message_seq, "checkpoint message seq")?,
        created_at: rows::time(created, "checkpoint time")?,
        status: match status.as_str() {
            "valid" => CheckpointStatus::Valid,
            "corrupt" => CheckpointStatus::Corrupt,
            _ => return Err(rows::corrupt("invalid checkpoint status")),
        },
        corrupt_reason: reason,
    })
}

pub(crate) fn load_metas(
    connection: &Connection,
    session_id: &EntityId,
) -> Result<Vec<CheckpointMeta>, SessionError> {
    let mut statement = connection.prepare(&format!(
        "SELECT {CHECKPOINT_COLUMNS} FROM checkpoints WHERE session_id = ?1 ORDER BY seq"
    ))?;
    let rows = statement
        .query_map([session_id.as_str()], checkpoint_row)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(decode_meta).collect()
}

fn load_meta(
    connection: &Connection,
    checkpoint_id: &CheckpointId,
) -> Result<CheckpointMeta, SessionError> {
    connection
        .query_row(
            &format!("SELECT {CHECKPOINT_COLUMNS} FROM checkpoints WHERE id = ?1"),
            [checkpoint_id.as_str()],
            checkpoint_row,
        )
        .optional()?
        .ok_or_else(|| SessionError::not_found("checkpoint", checkpoint_id))
        .and_then(decode_meta)
}

/// Why a checkpoint could not be restored.
enum Failure {
    /// Damaged: mark it corrupt and fall back.
    Corrupt(String),
    /// Not a property of the checkpoint (I/O, newer format): report it.
    Fatal(SessionError),
}

fn mark_corrupt(
    connection: &Connection,
    checkpoint_id: &CheckpointId,
    reason: &str,
) -> Result<(), SessionError> {
    connection.execute(
        "UPDATE checkpoints SET status = 'corrupt', corrupt_reason = ?2 WHERE id = ?1",
        params![checkpoint_id.as_str(), reason],
    )?;
    Ok(())
}

impl SessionStore {
    /// Read and verify one checkpoint blob.
    fn verify(&self, meta: &CheckpointMeta) -> Result<Checkpoint, Failure> {
        let bytes = match self.blobs.get_artifact(&meta.sha256) {
            Ok(bytes) => bytes,
            Err(StoreError::Corrupt(reason)) => return Err(Failure::Corrupt(reason)),
            Err(StoreError::InvalidKey) => {
                return Err(Failure::Corrupt("recorded hash is malformed".into()));
            }
            Err(other) => return Err(Failure::Fatal(other.into())),
        };
        if bytes.len() as u64 != meta.size {
            return Err(Failure::Corrupt("size does not match the record".into()));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Failure::Corrupt("envelope is not valid JSON".into()))?;
        match value.get("format").and_then(Value::as_u64) {
            Some(format) if format == u64::from(CHECKPOINT_FORMAT) => {}
            Some(format) if format > u64::from(CHECKPOINT_FORMAT) => {
                return Err(Failure::Fatal(SessionError::Incompatible(format!(
                    "checkpoint `{}` uses format {format}; this build reads format {CHECKPOINT_FORMAT} (upgrade AiDEX)",
                    meta.id
                ))));
            }
            _ => {
                return Err(Failure::Corrupt(
                    "envelope format is missing or invalid".into(),
                ));
            }
        }
        let envelope: Envelope = serde_json::from_value(value).map_err(|_| {
            Failure::Corrupt("envelope does not match the checkpoint schema".into())
        })?;
        if envelope.checkpoint_id != meta.id
            || envelope.session_id != meta.session_id
            || envelope.marks.message_seq != meta.message_seq
        {
            return Err(Failure::Corrupt(
                "envelope does not belong to this checkpoint record".into(),
            ));
        }
        Ok(Checkpoint {
            meta: meta.clone(),
            host_fingerprint: envelope.host,
            seeded_from: envelope.seeded_from,
            marks: envelope.marks,
            state: envelope.state,
        })
    }

    /// The newest checkpoint that verifies, falling back past corrupt ones.
    pub(crate) fn restore_latest(
        &self,
        connection: &Connection,
        session_id: &EntityId,
    ) -> Result<Option<RestoredCheckpoint>, SessionError> {
        let metas = load_metas(connection, session_id)?;
        if metas.is_empty() {
            return Ok(None);
        }
        let mut corrupt = Vec::new();
        for meta in metas.iter().rev() {
            if meta.status == CheckpointStatus::Corrupt {
                corrupt.push(meta.id.clone());
                continue;
            }
            match self.verify(meta) {
                Ok(checkpoint) => {
                    return Ok(Some(RestoredCheckpoint {
                        checkpoint,
                        recovered_from_corruption: !corrupt.is_empty(),
                        corrupt_checkpoints: corrupt,
                    }));
                }
                Err(Failure::Corrupt(reason)) => {
                    mark_corrupt(connection, &meta.id, &reason)?;
                    corrupt.push(meta.id.clone());
                }
                Err(Failure::Fatal(error)) => return Err(error),
            }
        }
        Err(SessionError::NoValidCheckpoint {
            session_id: session_id.to_string(),
            corrupt: corrupt.len(),
        })
    }

    /// Whether any checkpoint of the session verifies.
    pub(crate) fn has_valid_checkpoint(
        &self,
        connection: &Connection,
        session_id: &EntityId,
    ) -> Result<bool, SessionError> {
        match self.restore_latest(connection, session_id) {
            Ok(found) => Ok(found.is_some()),
            Err(SessionError::NoValidCheckpoint { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Capture marks and write the envelope; the caller commits and publishes.
    fn write_checkpoint(
        &self,
        transaction: &Transaction<'_>,
        session: &Session,
        label: Option<String>,
        marks: HighWaterMarks,
        state: LogicalState,
        seeded_from: Option<CheckpointId>,
    ) -> Result<(CheckpointMeta, Pending), SessionError> {
        let id = EntityId::generate("chk").map_err(|error| {
            SessionError::Unavailable(format!("cannot generate a checkpoint id: {error}"))
        })?;
        let created_at = Timestamp::now();
        let message_seq = marks.message_seq;
        let envelope = Envelope {
            format: CHECKPOINT_FORMAT,
            checkpoint_id: id.clone(),
            session_id: session.id.clone(),
            workspace_id: session.workspace_id.clone(),
            created_at: created_at.clone(),
            label: label.clone(),
            host: self.host().clone(),
            seeded_from,
            marks,
            state,
        };
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|error| SessionError::Storage(format!("cannot encode checkpoint: {error}")))?;
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(SessionError::InvalidArgument(format!(
                "checkpoint is {} bytes; the limit is {MAX_CHECKPOINT_BYTES}",
                bytes.len()
            )));
        }
        let sha256 = self.blobs.put_artifact(&bytes)?;
        let seq: i64 = transaction.query_row(
            "SELECT next_checkpoint_seq FROM sessions WHERE id = ?1",
            [session.id.as_str()],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO checkpoints (id, session_id, seq, label, sha256, size, format, message_seq, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'valid', ?9)",
            params![
                id.as_str(),
                session.id.as_str(),
                seq,
                label,
                sha256,
                signed(bytes.len() as u64),
                CHECKPOINT_FORMAT,
                signed(message_seq),
                created_at.as_str(),
            ],
        )?;
        transaction.execute(
            "UPDATE sessions SET next_checkpoint_seq = ?2, updated_at = ?3 WHERE id = ?1",
            params![
                session.id.as_str(),
                seq.saturating_add(1),
                created_at.as_str()
            ],
        )?;
        let pending = stage(
            transaction,
            &session.id,
            EventKind::SessionCheckpointed {
                checkpoint_id: id.clone(),
                label: label.clone(),
            },
        )?;
        let meta = CheckpointMeta {
            id,
            session_id: session.id.clone(),
            seq: rows::unsigned(seq, "checkpoint seq")?,
            label,
            sha256,
            size: bytes.len() as u64,
            format: CHECKPOINT_FORMAT,
            message_seq,
            created_at,
            status: CheckpointStatus::Valid,
            corrupt_reason: None,
        };
        Ok((meta, pending))
    }

    /// Write a checkpoint of the session's logical state (`session.checkpointed`).
    ///
    /// The store captures the high-water marks itself: last message seq, every tool
    /// call and its state, the event cursor, model, budget, and open approvals.
    pub fn checkpoint(
        &self,
        id: &EntityId,
        label: Option<&str>,
        state: LogicalState,
    ) -> Result<CheckpointMeta, SessionError> {
        let label = label
            .map(|label| {
                bounded("label", label, MAX_LABEL_BYTES, false).map(|label| redact(label).0)
            })
            .transpose()?;
        let (harness, _) = redact_json(state.harness);
        let task_graph = state.task_graph.map(|graph| redact_json(graph).0);
        let state = LogicalState {
            harness,
            task_graph,
        };
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut session = load_open_session(&transaction, id)?;
        let marks = capture_marks(&transaction, &session)?;
        let (meta, pending) =
            self.write_checkpoint(&transaction, &session, label, marks, state, None)?;
        transaction.commit()?;
        self.publish(&mut connection, &mut session, vec![pending]);
        Ok(meta)
    }

    /// Checkpoint records of the session, oldest first (blobs are not read).
    pub fn checkpoints(&self, id: &EntityId) -> Result<Vec<CheckpointMeta>, SessionError> {
        let connection = self.lock();
        load_session(&connection, id)?;
        load_metas(&connection, id)
    }

    /// The newest checkpoint that verifies, or `None` when the session has none.
    ///
    /// Corrupt checkpoints met on the way are marked corrupt; if every checkpoint is
    /// corrupt the result is [`SessionError::NoValidCheckpoint`].
    pub fn latest_valid_checkpoint(
        &self,
        id: &EntityId,
    ) -> Result<Option<RestoredCheckpoint>, SessionError> {
        let connection = self.lock();
        load_session(&connection, id)?;
        self.restore_latest(&connection, id)
    }

    /// Verify and decode one checkpoint of the session.
    pub fn load_checkpoint(
        &self,
        id: &EntityId,
        checkpoint_id: &CheckpointId,
    ) -> Result<Checkpoint, SessionError> {
        let connection = self.lock();
        load_session(&connection, id)?;
        self.load_owned_checkpoint(&connection, id, checkpoint_id)
    }

    fn load_owned_checkpoint(
        &self,
        connection: &Connection,
        id: &EntityId,
        checkpoint_id: &CheckpointId,
    ) -> Result<Checkpoint, SessionError> {
        let meta = load_meta(connection, checkpoint_id)?;
        if &meta.session_id != id {
            return Err(SessionError::not_found("checkpoint", checkpoint_id));
        }
        if meta.status == CheckpointStatus::Corrupt {
            return Err(SessionError::CheckpointCorrupt {
                checkpoint_id: checkpoint_id.to_string(),
                reason: meta
                    .corrupt_reason
                    .unwrap_or_else(|| "previously failed verification".into()),
            });
        }
        match self.verify(&meta) {
            Ok(checkpoint) => Ok(checkpoint),
            Err(Failure::Corrupt(reason)) => {
                mark_corrupt(connection, checkpoint_id, &reason)?;
                Err(SessionError::CheckpointCorrupt {
                    checkpoint_id: checkpoint_id.to_string(),
                    reason,
                })
            }
            Err(Failure::Fatal(error)) => Err(error),
        }
    }

    /// Branch a new child session from a verified checkpoint of `from` (spec §74).
    ///
    /// The child (state `created`, same workspace and agent, `parent_session_id =
    /// from`, `branched_from = checkpoint_id`, the checkpoint's model) gets a copy of
    /// the parent's messages up to the checkpoint and a seed checkpoint with the same
    /// logical state. Approvals, budget, tool calls, and the event cursor are not
    /// inherited: an approval granted to the parent never applies to a branch.
    pub fn branch(
        &self,
        from: &EntityId,
        checkpoint_id: &CheckpointId,
    ) -> Result<Session, SessionError> {
        let mut connection = self.lock();
        let parent = load_session(&connection, from)?;
        let source = self.load_owned_checkpoint(&connection, from, checkpoint_id)?;
        let now = Timestamp::now();
        let mut child = Session {
            id: EntityId::generate("ses").map_err(|error| {
                SessionError::Unavailable(format!("cannot generate a session id: {error}"))
            })?,
            workspace_id: parent.workspace_id.clone(),
            agent_id: parent.agent_id.clone(),
            title: branch_title(&parent.title),
            state: SessionState::Created,
            parent_session_id: Some(parent.id.clone()),
            branched_from: Some(checkpoint_id.clone()),
            model: source.marks.model.clone(),
            created_at: now.clone(),
            updated_at: now,
            host_fingerprint: self.host().clone(),
            last_event_cursor: Cursor::ZERO,
            budget: BudgetUsage::default(),
            resumed_at: None,
        };
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        insert_session(&transaction, &child)?;
        let created = stage(
            &transaction,
            &child.id,
            EventKind::SessionCreated {
                agent_id: Some(child.agent_id.clone()),
                title: Some(child.title.clone()),
            },
        )?;
        let copied = transaction.execute(
            "INSERT INTO messages (session_id, seq, role, content, tool_call_id, redacted, created_at)
             SELECT ?1, seq, role, content, tool_call_id, redacted, created_at FROM messages
             WHERE session_id = ?2 AND seq <= ?3",
            params![child.id.as_str(), from.as_str(), signed(source.marks.message_seq)],
        )?;
        if copied as u64 != source.marks.message_seq {
            return Err(SessionError::Corrupt(format!(
                "session `{from}` is missing messages up to checkpoint `{checkpoint_id}`"
            )));
        }
        transaction.execute(
            "UPDATE sessions SET next_message_seq = ?2 WHERE id = ?1",
            params![
                child.id.as_str(),
                signed(source.marks.message_seq).saturating_add(1)
            ],
        )?;
        let marks = HighWaterMarks {
            message_seq: source.marks.message_seq,
            tool_calls: source.marks.tool_calls.clone(),
            event_cursor: Cursor::ZERO,
            model: source.marks.model.clone(),
            open_approvals: Vec::new(),
            budget: BudgetUsage::default(),
            session_state: SessionStateKind::Created,
        };
        let (_, seeded) = self.write_checkpoint(
            &transaction,
            &child,
            Some(format!("branched from {checkpoint_id}")),
            marks,
            source.state,
            Some(checkpoint_id.clone()),
        )?;
        transaction.commit()?;
        self.publish(&mut connection, &mut child, vec![created, seeded]);
        Ok(child)
    }
}

fn capture_marks(
    connection: &Connection,
    session: &Session,
) -> Result<HighWaterMarks, SessionError> {
    let next: i64 = connection.query_row(
        "SELECT next_message_seq FROM sessions WHERE id = ?1",
        [session.id.as_str()],
        |row| row.get(0),
    )?;
    let now = Timestamp::now();
    Ok(HighWaterMarks {
        message_seq: rows::unsigned(next.saturating_sub(1), "message seq")?,
        tool_calls: load_tool_calls(connection, &session.id)?
            .into_iter()
            .map(|call| ToolCallMark {
                call_id: call.call_id,
                tool_id: call.tool_id,
                state: call.state,
            })
            .collect(),
        event_cursor: session.last_event_cursor,
        model: session.model.clone(),
        open_approvals: load_approvals(connection, &session.id)?
            .into_iter()
            .filter(|record| record.is_open(&now))
            .map(|record| record.approval.approval_id)
            .collect(),
        budget: session.budget,
        session_state: session.kind(),
    })
}

/// `branch of <title>`, cut on a char boundary to fit the title limit.
fn branch_title(parent: &str) -> String {
    const PREFIX: &str = "branch of ";
    let mut cut = parent.len().min(MAX_TITLE_BYTES - PREFIX.len());
    while !parent.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{PREFIX}{}", &parent[..cut])
}

impl CheckpointMeta {
    pub fn is_valid(&self) -> bool {
        self.status == CheckpointStatus::Valid
    }
}

/// The newest checkpoint record of a session (blob not read).
pub(crate) fn latest_meta(
    connection: &Connection,
    session_id: &EntityId,
) -> Result<Option<CheckpointMeta>, SessionError> {
    connection
        .query_row(
            &format!(
                "SELECT {CHECKPOINT_COLUMNS} FROM checkpoints WHERE session_id = ?1 ORDER BY seq DESC LIMIT 1"
            ),
            [session_id.as_str()],
            checkpoint_row,
        )
        .optional()?
        .map(decode_meta)
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_titles_fit_the_limit_on_char_boundaries() {
        assert_eq!(branch_title("fix bug"), "branch of fix bug");
        let long = "é".repeat(MAX_TITLE_BYTES);
        let title = branch_title(&long);
        assert!(title.len() <= MAX_TITLE_BYTES);
        assert!(title.starts_with("branch of é"));
    }

    #[test]
    fn envelope_rejects_unknown_fields() {
        let state: Result<LogicalState, _> =
            serde_json::from_value(serde_json::json!({"harness": {}, "extra": 1}));
        assert!(state.is_err());
    }
}
