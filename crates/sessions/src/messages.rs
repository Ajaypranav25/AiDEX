//! Conversation messages with a monotonic per-session sequence.

use aidex_core::{EntityId, Timestamp};
use aidex_events::MessageRole;
use rusqlite::{Connection, Row, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{
    SessionError, SessionStore,
    model::bounded,
    secrets::redact,
    store::rows::{self, load_open_session, signed, touch},
};

/// Longest message content, in bytes (larger output belongs in an artifact).
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024;
/// Most messages returned by one [`SessionStore::messages`] call.
pub const MAX_MESSAGE_PAGE: usize = 500;

/// A stored message. `seq` starts at 1 and increases by one per append.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub session_id: EntityId,
    pub seq: u64,
    pub role: MessageRole,
    /// Content after credential redaction.
    pub content: String,
    pub tool_call_id: Option<EntityId>,
    /// Whether redaction replaced anything in `content`.
    pub redacted: bool,
    pub created_at: Timestamp,
}

/// Input of [`SessionStore::append_message`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewMessage {
    pub role: MessageRole,
    pub content: String,
    /// The tool call a `tool` message answers.
    pub tool_call_id: Option<EntityId>,
}

impl NewMessage {
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
        }
    }

    #[must_use]
    pub fn for_tool_call(mut self, call_id: EntityId) -> Self {
        self.tool_call_id = Some(call_id);
        self
    }
}

/// One page of messages.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessagePage {
    pub messages: Vec<Message>,
    /// Pass as `after` to continue (the last returned `seq`, or the request's `after`).
    pub next_after: u64,
    pub has_more: bool,
}

pub(crate) fn role_name(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "system",
        MessageRole::Tool => "tool",
    }
}

fn parse_role(name: &str) -> Result<MessageRole, SessionError> {
    Ok(match name {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "system" => MessageRole::System,
        "tool" => MessageRole::Tool,
        _ => return Err(rows::corrupt("invalid message role")),
    })
}

type MessageRow = (String, i64, String, String, Option<String>, bool, String);

const MESSAGE_COLUMNS: &str = "session_id, seq, role, content, tool_call_id, redacted, created_at";

fn message_row(row: &Row<'_>) -> rusqlite::Result<MessageRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn decode(
    (session_id, seq, role, content, tool_call_id, redacted, created_at): MessageRow,
) -> Result<Message, SessionError> {
    Ok(Message {
        session_id: rows::id(session_id, "message session id")?,
        seq: rows::unsigned(seq, "message seq")?,
        role: parse_role(&role)?,
        content,
        tool_call_id: rows::opt_id(tool_call_id, "message tool call id")?,
        redacted,
        created_at: rows::time(created_at, "message time")?,
    })
}

/// Messages of `id` with `seq > after`, ascending, at most `limit`.
pub(crate) fn load_messages(
    connection: &Connection,
    id: &EntityId,
    after: u64,
    limit: Option<usize>,
) -> Result<Vec<Message>, SessionError> {
    let limit = limit.map_or(-1, |limit| signed(limit as u64));
    let mut statement = connection.prepare(&format!(
        "SELECT {MESSAGE_COLUMNS} FROM messages WHERE session_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3"
    ))?;
    let rows = statement.query_map(params![id.as_str(), signed(after), limit], message_row)?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(decode(row?)?);
    }
    Ok(messages)
}

/// Insert a message at the session's next sequence number.
pub(crate) fn insert_message(
    connection: &Connection,
    session_id: &EntityId,
    message: &NewMessage,
    created_at: &Timestamp,
) -> Result<Message, SessionError> {
    let (content, redacted) = redact(bounded(
        "message content",
        &message.content,
        MAX_MESSAGE_BYTES,
        true,
    )?);
    let seq: i64 = connection.query_row(
        "SELECT next_message_seq FROM sessions WHERE id = ?1",
        [session_id.as_str()],
        |row| row.get(0),
    )?;
    connection.execute(
        "INSERT INTO messages (session_id, seq, role, content, tool_call_id, redacted, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            session_id.as_str(),
            seq,
            role_name(message.role),
            content,
            message.tool_call_id.as_ref().map(EntityId::as_str),
            redacted,
            created_at.as_str(),
        ],
    )?;
    connection.execute(
        "UPDATE sessions SET next_message_seq = ?2 WHERE id = ?1",
        params![session_id.as_str(), seq.saturating_add(1)],
    )?;
    Ok(Message {
        session_id: session_id.clone(),
        seq: rows::unsigned(seq, "message seq")?,
        role: message.role,
        content,
        tool_call_id: message.tool_call_id.clone(),
        redacted,
        created_at: created_at.clone(),
    })
}

impl SessionStore {
    /// Append a message; content is redacted and limited to [`MAX_MESSAGE_BYTES`].
    /// Sequence numbers are assigned in one transaction, so they are gap-free and
    /// strictly increasing per session.
    pub fn append_message(
        &self,
        id: &EntityId,
        message: NewMessage,
    ) -> Result<Message, SessionError> {
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        load_open_session(&transaction, id)?;
        let now = Timestamp::now();
        let stored = insert_message(&transaction, id, &message, &now)?;
        touch(&transaction, id, &now)?;
        transaction.commit()?;
        Ok(stored)
    }

    /// Messages with `seq > after`, ascending; `limit` from 1 to [`MAX_MESSAGE_PAGE`].
    pub fn messages(
        &self,
        id: &EntityId,
        after: u64,
        limit: usize,
    ) -> Result<MessagePage, SessionError> {
        if !(1..=MAX_MESSAGE_PAGE).contains(&limit) {
            return Err(SessionError::InvalidArgument(format!(
                "limit {limit} must be between 1 and {MAX_MESSAGE_PAGE}"
            )));
        }
        let connection = self.lock();
        if !Self::exists(&connection, id)? {
            return Err(SessionError::not_found("session", id));
        }
        let mut messages = load_messages(&connection, id, after, Some(limit + 1))?;
        let has_more = messages.len() > limit;
        messages.truncate(limit);
        let next_after = messages.last().map_or(after, |message| message.seq);
        Ok(MessagePage {
            messages,
            next_after,
            has_more,
        })
    }
}
