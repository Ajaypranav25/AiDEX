//! Compact per-session overview for `session.show`.

use aidex_core::EntityId;
use serde::{Deserialize, Serialize};

use crate::{
    CheckpointMeta, EventGap, Session, SessionError, SessionStore,
    checkpoints::latest_meta,
    recovery::load_gaps,
    store::rows::{self, load_session},
};

/// A session with record counts, its newest checkpoint record, and event gaps.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSummary {
    pub session: Session,
    pub messages: u64,
    pub tool_calls: u64,
    /// Requested, awaiting approval, or running.
    pub open_tool_calls: u64,
    pub artifacts: u64,
    pub checkpoints: u64,
    pub approvals: u64,
    /// The newest checkpoint record (not verified; see `latest_valid_checkpoint`).
    pub latest_checkpoint: Option<CheckpointMeta>,
    pub event_gaps: Vec<EventGap>,
}

impl SessionStore {
    pub fn summary(&self, id: &EntityId) -> Result<SessionSummary, SessionError> {
        let connection = self.lock();
        let session = load_session(&connection, id)?;
        let open: i64 = connection.query_row(
            "SELECT COUNT(*) FROM tool_calls WHERE session_id = ?1 AND state IN ('requested', 'awaiting_approval', 'running')",
            [id.as_str()],
            |row| row.get(0),
        )?;
        Ok(SessionSummary {
            messages: Self::count(&connection, "messages", id)?,
            tool_calls: Self::count(&connection, "tool_calls", id)?,
            open_tool_calls: rows::unsigned(open, "tool call count")?,
            artifacts: Self::count(&connection, "artifacts", id)?,
            checkpoints: Self::count(&connection, "checkpoints", id)?,
            approvals: Self::count(&connection, "approvals", id)?,
            latest_checkpoint: latest_meta(&connection, id)?,
            event_gaps: load_gaps(&connection, id)?,
            session,
        })
    }
}
