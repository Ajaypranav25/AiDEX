//! Tool calls and their results, as recorded for provenance and resume.
//!
//! Inputs and summaries are redacted before they are written. Host-bound process
//! state is never serialized: a call records only the opaque `process_id` the
//! sandbox assigned, and a call still `running` at startup becomes `lost`.

use aidex_core::{EntityId, Timestamp};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ArtifactRef, SessionError, SessionStore,
    artifacts::load_artifact,
    model::bounded,
    secrets::{redact, redact_json},
    store::rows::{self, encode, load_open_session, load_session, touch},
};

/// Longest encoded tool input, in bytes.
pub const MAX_TOOL_INPUT_BYTES: usize = 256 * 1024;
/// Longest result summary, in bytes.
pub const MAX_SUMMARY_BYTES: usize = 16 * 1024;

/// Lifecycle of one tool call.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallState {
    Requested,
    AwaitingApproval,
    Running,
    Completed,
    Failed,
    Denied,
    Cancelled,
    /// Its process belonged to a daemon that stopped; the outcome is unknown.
    Lost,
}

impl ToolCallState {
    pub const ALL: [Self; 8] = [
        Self::Requested,
        Self::AwaitingApproval,
        Self::Running,
        Self::Completed,
        Self::Failed,
        Self::Denied,
        Self::Cancelled,
        Self::Lost,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
            Self::Lost => "lost",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Denied | Self::Cancelled | Self::Lost
        )
    }

    fn parse(name: &str) -> Result<Self, SessionError> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_str() == name)
            .ok_or_else(|| rows::corrupt("invalid tool call state"))
    }
}

/// Input of [`SessionStore::record_tool_call`].
#[derive(Clone, Debug, PartialEq)]
pub struct NewToolCall {
    /// Caller-assigned, unique within the session.
    pub call_id: EntityId,
    pub tool_id: EntityId,
    /// Normalized tool input; redacted before it is stored. Never put credentials
    /// here; reference them by vault handle.
    pub input: Value,
    pub state: ToolCallState,
    pub approval_id: Option<EntityId>,
    /// Sandbox process id (`proc_…`) of a running call; never restored.
    pub process_id: Option<EntityId>,
    pub started_at: Option<Timestamp>,
}

impl NewToolCall {
    pub fn new(call_id: EntityId, tool_id: EntityId, input: Value) -> Self {
        Self {
            call_id,
            tool_id,
            input,
            state: ToolCallState::Requested,
            approval_id: None,
            process_id: None,
            started_at: None,
        }
    }
}

/// Change to a recorded call. `None` fields keep their stored value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolCallUpdate {
    pub state: Option<ToolCallState>,
    pub result_summary: Option<String>,
    /// An artifact of the same session holding the full output.
    pub output_artifact: Option<EntityId>,
    pub approval_id: Option<EntityId>,
    pub process_id: Option<EntityId>,
    /// Defaults to now when the call reaches a terminal state.
    pub finished_at: Option<Timestamp>,
}

impl ToolCallUpdate {
    pub fn state(state: ToolCallState) -> Self {
        Self {
            state: Some(state),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.result_summary = Some(summary.into());
        self
    }

    #[must_use]
    pub fn with_artifact(mut self, artifact_id: EntityId) -> Self {
        self.output_artifact = Some(artifact_id);
        self
    }

    #[must_use]
    pub fn with_process(mut self, process_id: EntityId) -> Self {
        self.process_id = Some(process_id);
        self
    }
}

/// A stored tool call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallRecord {
    pub session_id: EntityId,
    pub call_id: EntityId,
    pub tool_id: EntityId,
    /// Input after credential redaction.
    pub input: Value,
    pub state: ToolCallState,
    pub result_summary: Option<String>,
    pub output_artifact: Option<ArtifactRef>,
    pub approval_id: Option<EntityId>,
    pub process_id: Option<EntityId>,
    /// Whether redaction replaced anything in `input` or `result_summary`.
    pub redacted: bool,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

const TOOL_COLUMNS: &str = "session_id, call_id, tool_id, input_json, state, result_summary, \
    output_artifact_id, approval_id, process_id, redacted, started_at, finished_at";

struct ToolRow {
    session_id: String,
    call_id: String,
    tool_id: String,
    input_json: String,
    state: String,
    result_summary: Option<String>,
    output_artifact_id: Option<String>,
    approval_id: Option<String>,
    process_id: Option<String>,
    redacted: bool,
    started_at: Option<String>,
    finished_at: Option<String>,
}

fn tool_row(row: &Row<'_>) -> rusqlite::Result<ToolRow> {
    Ok(ToolRow {
        session_id: row.get(0)?,
        call_id: row.get(1)?,
        tool_id: row.get(2)?,
        input_json: row.get(3)?,
        state: row.get(4)?,
        result_summary: row.get(5)?,
        output_artifact_id: row.get(6)?,
        approval_id: row.get(7)?,
        process_id: row.get(8)?,
        redacted: row.get(9)?,
        started_at: row.get(10)?,
        finished_at: row.get(11)?,
    })
}

fn decode(connection: &Connection, row: ToolRow) -> Result<ToolCallRecord, SessionError> {
    let output_artifact = rows::opt_id(row.output_artifact_id, "tool call artifact")?
        .map(|artifact| load_artifact(connection, &artifact))
        .transpose()?;
    Ok(ToolCallRecord {
        session_id: rows::id(row.session_id, "tool call session id")?,
        call_id: rows::id(row.call_id, "tool call id")?,
        tool_id: rows::id(row.tool_id, "tool id")?,
        input: rows::json(&row.input_json, "tool call input")?,
        state: ToolCallState::parse(&row.state)?,
        result_summary: row.result_summary,
        output_artifact,
        approval_id: rows::opt_id(row.approval_id, "tool call approval id")?,
        process_id: rows::opt_id(row.process_id, "tool call process id")?,
        redacted: row.redacted,
        started_at: rows::opt_time(row.started_at, "tool call start")?,
        finished_at: rows::opt_time(row.finished_at, "tool call finish")?,
    })
}

pub(crate) fn load_tool_call(
    connection: &Connection,
    session_id: &EntityId,
    call_id: &EntityId,
) -> Result<ToolCallRecord, SessionError> {
    let row = connection
        .query_row(
            &format!(
                "SELECT {TOOL_COLUMNS} FROM tool_calls WHERE session_id = ?1 AND call_id = ?2"
            ),
            params![session_id.as_str(), call_id.as_str()],
            tool_row,
        )
        .optional()?
        .ok_or_else(|| SessionError::not_found("tool call", call_id))?;
    decode(connection, row)
}

/// Every call of a session in recording order.
pub(crate) fn load_tool_calls(
    connection: &Connection,
    session_id: &EntityId,
) -> Result<Vec<ToolCallRecord>, SessionError> {
    let mut statement = connection.prepare(&format!(
        "SELECT {TOOL_COLUMNS} FROM tool_calls WHERE session_id = ?1 ORDER BY ordinal"
    ))?;
    let rows = statement
        .query_map([session_id.as_str()], tool_row)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|row| decode(connection, row))
        .collect()
}

fn clean_input(input: Value) -> Result<(Value, bool), SessionError> {
    let encoded = encode(&input)?;
    if encoded.len() > MAX_TOOL_INPUT_BYTES {
        return Err(SessionError::InvalidArgument(format!(
            "tool input is {} bytes; the limit is {MAX_TOOL_INPUT_BYTES} (store large payloads as artifacts)",
            encoded.len()
        )));
    }
    Ok(redact_json(input))
}

/// Ensure an artifact exists and belongs to `session_id`.
fn own_artifact(
    connection: &Connection,
    session_id: &EntityId,
    artifact_id: &EntityId,
) -> Result<(), SessionError> {
    let artifact = load_artifact(connection, artifact_id)?;
    if &artifact.session_id != session_id {
        return Err(SessionError::InvalidArgument(format!(
            "artifact `{artifact_id}` belongs to another session"
        )));
    }
    Ok(())
}

impl SessionStore {
    /// Record a new tool call. A duplicate `call_id` in the session is a conflict.
    pub fn record_tool_call(
        &self,
        id: &EntityId,
        call: NewToolCall,
    ) -> Result<ToolCallRecord, SessionError> {
        let (input, redacted) = clean_input(call.input)?;
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        load_open_session(&transaction, id)?;
        let exists = transaction
            .query_row(
                "SELECT 1 FROM tool_calls WHERE session_id = ?1 AND call_id = ?2",
                params![id.as_str(), call.call_id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            return Err(SessionError::Conflict(format!(
                "tool call `{}` is already recorded",
                call.call_id
            )));
        }
        let ordinal: i64 = transaction.query_row(
            "SELECT next_tool_ordinal FROM sessions WHERE id = ?1",
            [id.as_str()],
            |row| row.get(0),
        )?;
        let now = Timestamp::now();
        let started_at = call
            .started_at
            .or_else(|| (call.state == ToolCallState::Running).then(|| now.clone()));
        let finished_at = call.state.is_terminal().then(|| now.clone());
        transaction.execute(
            "INSERT INTO tool_calls (session_id, call_id, ordinal, tool_id, input_json, state, approval_id,
                process_id, redacted, started_at, finished_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id.as_str(),
                call.call_id.as_str(),
                ordinal,
                call.tool_id.as_str(),
                encode(&input)?,
                call.state.as_str(),
                call.approval_id.as_ref().map(EntityId::as_str),
                call.process_id.as_ref().map(EntityId::as_str),
                redacted,
                started_at.as_ref().map(Timestamp::as_str),
                finished_at.as_ref().map(Timestamp::as_str),
            ],
        )?;
        transaction.execute(
            "UPDATE sessions SET next_tool_ordinal = ?2 WHERE id = ?1",
            params![id.as_str(), ordinal.saturating_add(1)],
        )?;
        touch(&transaction, id, &now)?;
        let record = load_tool_call(&transaction, id, &call.call_id)?;
        transaction.commit()?;
        Ok(record)
    }

    /// Update a recorded call. A call in a terminal state cannot change (conflict).
    /// Updates are accepted after the session itself closed, so late results are kept.
    pub fn update_tool_call(
        &self,
        id: &EntityId,
        call_id: &EntityId,
        update: ToolCallUpdate,
    ) -> Result<ToolCallRecord, SessionError> {
        let summary = update
            .result_summary
            .as_deref()
            .map(|summary| bounded("result summary", summary, MAX_SUMMARY_BYTES, true).map(redact))
            .transpose()?;
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        load_session(&transaction, id)?;
        let current = load_tool_call(&transaction, id, call_id)?;
        if current.state.is_terminal() {
            return Err(SessionError::Conflict(format!(
                "tool call `{call_id}` already finished ({})",
                current.state.as_str()
            )));
        }
        if let Some(artifact) = &update.output_artifact {
            own_artifact(&transaction, id, artifact)?;
        }
        let now = Timestamp::now();
        let state = update.state.unwrap_or(current.state);
        let started_at = current
            .started_at
            .or_else(|| (state == ToolCallState::Running).then(|| now.clone()));
        let finished_at = update
            .finished_at
            .or_else(|| state.is_terminal().then(|| now.clone()));
        let redacted = current.redacted || summary.as_ref().is_some_and(|(_, changed)| *changed);
        transaction.execute(
            "UPDATE tool_calls SET state = ?3,
                result_summary = COALESCE(?4, result_summary),
                output_artifact_id = COALESCE(?5, output_artifact_id),
                approval_id = COALESCE(?6, approval_id),
                process_id = COALESCE(?7, process_id),
                redacted = ?8, started_at = ?9, finished_at = ?10
             WHERE session_id = ?1 AND call_id = ?2",
            params![
                id.as_str(),
                call_id.as_str(),
                state.as_str(),
                summary.as_ref().map(|(text, _)| text.as_str()),
                update.output_artifact.as_ref().map(EntityId::as_str),
                update.approval_id.as_ref().map(EntityId::as_str),
                update.process_id.as_ref().map(EntityId::as_str),
                redacted,
                started_at.as_ref().map(Timestamp::as_str),
                finished_at.as_ref().map(Timestamp::as_str),
            ],
        )?;
        touch(&transaction, id, &now)?;
        let record = load_tool_call(&transaction, id, call_id)?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn tool_call(
        &self,
        id: &EntityId,
        call_id: &EntityId,
    ) -> Result<ToolCallRecord, SessionError> {
        let connection = self.lock();
        load_session(&connection, id)?;
        load_tool_call(&connection, id, call_id)
    }

    /// Every tool call of the session in recording order.
    pub fn tool_calls(&self, id: &EntityId) -> Result<Vec<ToolCallRecord>, SessionError> {
        let connection = self.lock();
        load_session(&connection, id)?;
        load_tool_calls(&connection, id)
    }
}

/// Mark every `running` call `lost`; returns `(session, call, process)` per call.
pub(crate) fn mark_running_lost(
    connection: &Connection,
    detail: &str,
) -> Result<Vec<(EntityId, EntityId, Option<EntityId>)>, SessionError> {
    let mut statement = connection.prepare(
        "SELECT session_id, call_id, process_id FROM tool_calls WHERE state = 'running' ORDER BY session_id, ordinal",
    )?;
    let found = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let now = Timestamp::now();
    let mut lost = Vec::with_capacity(found.len());
    for (session, call, process) in found {
        connection.execute(
            "UPDATE tool_calls SET state = 'lost', finished_at = ?3,
                result_summary = COALESCE(result_summary, ?4)
             WHERE session_id = ?1 AND call_id = ?2",
            params![session, call, now.as_str(), detail],
        )?;
        lost.push((
            rows::id(session, "tool call session id")?,
            rows::id(call, "tool call id")?,
            rows::opt_id(process, "tool call process id")?,
        ));
    }
    Ok(lost)
}
