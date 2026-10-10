//! Startup recovery after a crash or unclean stop (spec §60, §87).
//!
//! Nothing a previous daemon was doing survives it: no process, approval prompt, or
//! model state. Recovery makes that explicit instead of pretending otherwise:
//! active sessions become `interrupted` (recoverable when a checkpoint verifies),
//! running tool calls become `lost`, pending approvals are cancelled, and every
//! event the store cannot prove was recorded becomes an explicit [`EventGap`].
//! Missing events are never synthesised.

use aidex_core::{EntityId, Timestamp};
use aidex_events::Cursor;
use rusqlite::{Connection, Row, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{
    CheckpointId, HostFingerprint, SessionError, SessionState, SessionStateKind, SessionStore,
    approvals::cancel_pending,
    store::{
        publish::{GapRecord, record_gap},
        rows::{self, cursor_sql, load_session},
    },
    tool_calls::mark_running_lost,
};

/// Reason recorded on sessions interrupted by startup recovery.
pub const RESTART_REASON: &str = "daemon restarted";

/// Why some of a session's events may be missing from the event log.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventGapKind {
    /// The sink refused or failed the append; the state change itself committed.
    EmitFailed,
    /// The process stopped between committing a change and appending its event;
    /// the event may or may not be in the log.
    Interrupted,
    /// The session's recorded cursor is ahead of the event log (a replaced,
    /// restored, or truncated log); events up to `session_cursor` are not there.
    LogBehind,
}

impl EventGapKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmitFailed => "emit_failed",
            Self::Interrupted => "interrupted",
            Self::LogBehind => "log_behind",
        }
    }

    fn parse(name: &str) -> Result<Self, SessionError> {
        Ok(match name {
            "emit_failed" => Self::EmitFailed,
            "interrupted" => Self::Interrupted,
            "log_behind" => Self::LogBehind,
            _ => return Err(rows::corrupt("invalid event gap kind")),
        })
    }
}

/// An explicitly recorded hole in a session's event history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventGap {
    pub session_id: EntityId,
    pub kind: EventGapKind,
    /// The event name involved, when known.
    pub event: Option<String>,
    pub detail: String,
    pub session_cursor: Option<Cursor>,
    pub log_cursor: Option<Cursor>,
    pub detected_at: Timestamp,
}

/// A session that recovery moved to `interrupted`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterruptedSession {
    pub session_id: EntityId,
    pub workspace_id: EntityId,
    pub previous_state: SessionStateKind,
    pub recoverable: bool,
    /// The checkpoint a resume would start from.
    pub checkpoint_id: Option<CheckpointId>,
    /// The session last ran on another host.
    pub different_host: bool,
}

/// A tool call whose process belonged to the previous daemon.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LostToolCall {
    pub session_id: EntityId,
    pub call_id: EntityId,
    pub process_id: Option<EntityId>,
}

/// An approval that was pending when its session was interrupted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelledApproval {
    pub session_id: EntityId,
    pub approval_id: EntityId,
}

/// What [`SessionStore::recover_on_startup`] found and changed.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryReport {
    pub interrupted: Vec<InterruptedSession>,
    pub lost_tool_calls: Vec<LostToolCall>,
    pub cancelled_approvals: Vec<CancelledApproval>,
    pub event_gaps: Vec<EventGap>,
    /// Checkpoints that failed verification while deciding recoverability.
    pub corrupt_checkpoints: Vec<CheckpointId>,
}

impl RecoveryReport {
    /// Nothing needed recovery.
    pub fn is_clean(&self) -> bool {
        self.interrupted.is_empty()
            && self.lost_tool_calls.is_empty()
            && self.cancelled_approvals.is_empty()
            && self.event_gaps.is_empty()
            && self.corrupt_checkpoints.is_empty()
    }
}

const GAP_COLUMNS: &str =
    "session_id, kind, event_name, detail, session_cursor, log_cursor, detected_at";

type GapRow = (
    String,
    String,
    Option<String>,
    String,
    Option<i64>,
    Option<i64>,
    String,
);

fn gap_row(row: &Row<'_>) -> rusqlite::Result<GapRow> {
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

fn decode_gap(
    (session, kind, event, detail, session_cursor, log_cursor, detected): GapRow,
) -> Result<EventGap, SessionError> {
    let cursor = |value: Option<i64>| {
        value
            .map(|value| rows::unsigned(value, "gap cursor").map(Cursor::new))
            .transpose()
    };
    Ok(EventGap {
        session_id: rows::id(session, "gap session id")?,
        kind: EventGapKind::parse(&kind)?,
        event,
        detail,
        session_cursor: cursor(session_cursor)?,
        log_cursor: cursor(log_cursor)?,
        detected_at: rows::time(detected, "gap time")?,
    })
}

pub(crate) fn load_gaps(
    connection: &Connection,
    session_id: &EntityId,
) -> Result<Vec<EventGap>, SessionError> {
    let mut statement = connection.prepare(&format!(
        "SELECT {GAP_COLUMNS} FROM event_gaps WHERE session_id = ?1 ORDER BY id"
    ))?;
    let rows = statement
        .query_map([session_id.as_str()], gap_row)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(decode_gap).collect()
}

impl SessionStore {
    /// Recover after an unclean stop. Run once at daemon startup, before serving.
    ///
    /// - `pending_events` left by a crash between commit and append become
    ///   [`EventGapKind::Interrupted`] gaps (the events are not re-emitted);
    /// - when `event_log_latest` is known, sessions whose cursor is ahead of the log
    ///   get a [`EventGapKind::LogBehind`] gap and their cursor is lowered to it;
    /// - every `running` tool call becomes `lost` (processes are recreated, never
    ///   serialized);
    /// - every `running`/`waiting_approval` session has its pending approvals
    ///   cancelled and becomes `interrupted { reason: "daemon restarted",
    ///   recoverable }`, where `recoverable` means a checkpoint verified.
    ///
    /// `current_host` only informs `different_host` in the report. Idempotent: a
    /// second run finds nothing to do.
    pub fn recover_on_startup(
        &self,
        current_host: &HostFingerprint,
        event_log_latest: Option<Cursor>,
    ) -> Result<RecoveryReport, SessionError> {
        let mut report = RecoveryReport::default();
        let mut connection = self.lock();
        {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            report
                .event_gaps
                .extend(settle_interrupted_appends(&transaction)?);
            if let Some(latest) = event_log_latest {
                report
                    .event_gaps
                    .extend(reconcile_cursors(&transaction, latest)?);
            }
            report.lost_tool_calls = mark_running_lost(
                &transaction,
                "lost: the daemon stopped while this call was running; its process was not restored",
            )?
            .into_iter()
            .map(|(session_id, call_id, process_id)| LostToolCall {
                session_id,
                call_id,
                process_id,
            })
            .collect();
            transaction.commit()?;
        }
        for id in active_sessions(&connection)? {
            let session = load_session(&connection, &id)?;
            let restored = match self.restore_latest(&connection, &id) {
                Ok(restored) => restored,
                Err(SessionError::NoValidCheckpoint { .. }) => None,
                Err(error) => return Err(error),
            };
            let corrupt = crate::checkpoints::load_metas(&connection, &id)?
                .into_iter()
                .filter(|meta| !meta.is_valid())
                .map(|meta| meta.id);
            report.corrupt_checkpoints.extend(corrupt);
            for approval_id in cancel_pending(&connection, &id)? {
                report.cancelled_approvals.push(CancelledApproval {
                    session_id: id.clone(),
                    approval_id,
                });
            }
            let recoverable = restored.is_some();
            self.transition_locked(
                &mut connection,
                &id,
                SessionState::Interrupted {
                    reason: RESTART_REASON.into(),
                    recoverable,
                },
                None,
            )?;
            report.interrupted.push(InterruptedSession {
                session_id: id,
                previous_state: session.kind(),
                workspace_id: session.workspace_id,
                recoverable,
                checkpoint_id: restored.map(|restored| restored.checkpoint.meta.id),
                different_host: session.host_fingerprint.id != current_host.id,
            });
        }
        Ok(report)
    }

    /// Recorded event gaps of a session, oldest first.
    pub fn event_gaps(&self, id: &EntityId) -> Result<Vec<EventGap>, SessionError> {
        let connection = self.lock();
        load_session(&connection, id)?;
        load_gaps(&connection, id)
    }
}

fn active_sessions(connection: &Connection) -> Result<Vec<EntityId>, SessionError> {
    let mut statement = connection.prepare(
        "SELECT id FROM sessions WHERE state IN ('running', 'waiting_approval') ORDER BY rowid",
    )?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.into_iter()
        .map(|id| rows::id(id, "session id"))
        .collect()
}

/// Turn leftover outbox rows into `interrupted` gaps.
fn settle_interrupted_appends(connection: &Connection) -> Result<Vec<EventGap>, SessionError> {
    let mut statement = connection
        .prepare("SELECT id, session_id, event_name, created_at FROM pending_events ORDER BY id")?;
    let pending = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut gaps = Vec::with_capacity(pending.len());
    for (row_id, session, event, staged_at) in pending {
        let session = rows::id(session, "pending event session id")?;
        gaps.push(record_gap(
            connection,
            &session,
            GapRecord {
                kind: EventGapKind::Interrupted,
                event: Some(&event),
                detail: &format!(
                    "the daemon stopped after recording a change at {staged_at} and before confirming its event; the event may be missing from the log"
                ),
                session_cursor: None,
                log_cursor: None,
            },
        )?);
        connection.execute("DELETE FROM pending_events WHERE id = ?1", [row_id])?;
    }
    Ok(gaps)
}

/// Record and repair session cursors that are ahead of the event log.
fn reconcile_cursors(
    connection: &Connection,
    latest: Cursor,
) -> Result<Vec<EventGap>, SessionError> {
    let mut statement = connection.prepare(
        "SELECT id, last_event_cursor FROM sessions WHERE last_event_cursor > ?1 ORDER BY rowid",
    )?;
    let ahead = statement
        .query_map([cursor_sql(latest)], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut gaps = Vec::with_capacity(ahead.len());
    for (session, recorded) in ahead {
        let session = rows::id(session, "session id")?;
        let recorded = Cursor::new(rows::unsigned(recorded, "session cursor")?);
        gaps.push(record_gap(
            connection,
            &session,
            GapRecord {
                kind: EventGapKind::LogBehind,
                event: None,
                detail: &format!(
                    "the session recorded events up to cursor {recorded}, but the event log ends at {latest}; the missing events were not recreated"
                ),
                session_cursor: Some(recorded),
                log_cursor: Some(latest),
            },
        )?);
        connection.execute(
            "UPDATE sessions SET last_event_cursor = ?2 WHERE id = ?1",
            params![session.as_str(), cursor_sql(latest)],
        )?;
    }
    Ok(gaps)
}
