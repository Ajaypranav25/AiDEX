//! Column encoding helpers shared by the store modules.

use aidex_core::{EntityId, Timestamp};
use aidex_events::Cursor;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    BudgetUsage, HostFingerprint, Session, SessionError, SessionState,
    model::{MAX_REASON_BYTES, MAX_TITLE_BYTES},
};

/// Columns read by [`decode_session`], in order.
pub(crate) const SESSION_COLUMNS: &str = "id, workspace_id, agent_id, title, state_json, \
    parent_session_id, branched_from, model_json, created_at, updated_at, host_json, \
    last_event_cursor, input_tokens, output_tokens, tool_call_count, wall_ms, resumed_at";

pub(crate) fn corrupt(what: impl std::fmt::Display) -> SessionError {
    SessionError::Corrupt(what.to_string())
}

pub(crate) fn id(value: String, what: &str) -> Result<EntityId, SessionError> {
    EntityId::new(value).map_err(|_| corrupt(format!("invalid {what}")))
}

pub(crate) fn opt_id(value: Option<String>, what: &str) -> Result<Option<EntityId>, SessionError> {
    value.map(|value| id(value, what)).transpose()
}

pub(crate) fn time(value: String, what: &str) -> Result<Timestamp, SessionError> {
    Timestamp::new(value).map_err(|_| corrupt(format!("invalid {what}")))
}

pub(crate) fn opt_time(
    value: Option<String>,
    what: &str,
) -> Result<Option<Timestamp>, SessionError> {
    value.map(|value| time(value, what)).transpose()
}

pub(crate) fn json<T: DeserializeOwned>(value: &str, what: &str) -> Result<T, SessionError> {
    serde_json::from_str(value).map_err(|_| corrupt(format!("invalid {what}")))
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<String, SessionError> {
    serde_json::to_string(value)
        .map_err(|error| SessionError::Storage(format!("cannot encode record: {error}")))
}

pub(crate) fn unsigned(value: i64, what: &str) -> Result<u64, SessionError> {
    u64::try_from(value).map_err(|_| corrupt(format!("negative {what}")))
}

/// SQLite integers are signed; counters saturate at `i64::MAX`.
pub(crate) fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(crate) fn cursor_sql(cursor: Cursor) -> i64 {
    signed(cursor.get())
}

/// Raw session row; decoded outside rusqlite's callback so errors stay typed.
pub(crate) struct SessionRow {
    id: String,
    workspace_id: String,
    agent_id: String,
    title: String,
    state_json: String,
    parent_session_id: Option<String>,
    branched_from: Option<String>,
    model_json: Option<String>,
    created_at: String,
    updated_at: String,
    host_json: String,
    last_event_cursor: i64,
    input_tokens: i64,
    output_tokens: i64,
    tool_call_count: i64,
    wall_ms: i64,
    resumed_at: Option<String>,
}

pub(crate) fn session_row(row: &Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        agent_id: row.get(2)?,
        title: row.get(3)?,
        state_json: row.get(4)?,
        parent_session_id: row.get(5)?,
        branched_from: row.get(6)?,
        model_json: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        host_json: row.get(10)?,
        last_event_cursor: row.get(11)?,
        input_tokens: row.get(12)?,
        output_tokens: row.get(13)?,
        tool_call_count: row.get(14)?,
        wall_ms: row.get(15)?,
        resumed_at: row.get(16)?,
    })
}

impl SessionRow {
    pub(crate) fn decode(self) -> Result<Session, SessionError> {
        let what = |field: &str| format!("session `{}` {field}", self.id);
        let state: SessionState = json(&self.state_json, &what("state"))?;
        let host: HostFingerprint = json(&self.host_json, &what("host fingerprint"))?;
        host.validate()?;
        if self.title.len() > MAX_TITLE_BYTES {
            return Err(corrupt(what("title")));
        }
        let reason_len = match &state {
            SessionState::Failed { message: text, .. }
            | SessionState::Interrupted { reason: text, .. } => text.len(),
            _ => 0,
        };
        if reason_len > MAX_REASON_BYTES {
            return Err(corrupt(what("state")));
        }
        Ok(Session {
            workspace_id: id(self.workspace_id, &what("workspace id"))?,
            agent_id: id(self.agent_id, &what("agent id"))?,
            title: self.title,
            state,
            parent_session_id: opt_id(self.parent_session_id, &what("parent id"))?,
            branched_from: opt_id(self.branched_from, &what("branch checkpoint"))?,
            model: self
                .model_json
                .as_deref()
                .map(|model| json(model, &what("model")))
                .transpose()?,
            created_at: time(self.created_at, &what("created_at"))?,
            updated_at: time(self.updated_at, &what("updated_at"))?,
            host_fingerprint: host,
            last_event_cursor: Cursor::new(unsigned(self.last_event_cursor, &what("cursor"))?),
            budget: BudgetUsage {
                input_tokens: unsigned(self.input_tokens, &what("input tokens"))?,
                output_tokens: unsigned(self.output_tokens, &what("output tokens"))?,
                tool_calls: unsigned(self.tool_call_count, &what("tool calls"))?,
                wall_ms: unsigned(self.wall_ms, &what("wall time"))?,
            },
            resumed_at: opt_time(self.resumed_at, &what("resumed_at"))?,
            id: id(self.id, "session id")?,
        })
    }
}

/// Load one session or report it missing.
pub(crate) fn load_session(
    connection: &Connection,
    id: &EntityId,
) -> Result<Session, SessionError> {
    connection
        .query_row(
            &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
            params![id.as_str()],
            session_row,
        )
        .optional()?
        .ok_or_else(|| SessionError::not_found("session", id))?
        .decode()
}

/// Load a session that must still accept new records.
pub(crate) fn load_open_session(
    connection: &Connection,
    id: &EntityId,
) -> Result<Session, SessionError> {
    let session = load_session(connection, id)?;
    if session.state.is_terminal() {
        return Err(SessionError::SessionClosed {
            id: id.to_string(),
            state: session.kind(),
        });
    }
    Ok(session)
}

/// Bump `updated_at`.
pub(crate) fn touch(
    connection: &Connection,
    id: &EntityId,
    at: &Timestamp,
) -> Result<(), SessionError> {
    connection.execute(
        "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
        params![id.as_str(), at.as_str()],
    )?;
    Ok(())
}
