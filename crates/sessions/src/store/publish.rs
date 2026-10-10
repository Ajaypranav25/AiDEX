//! Event emission after commit, with an outbox so a lost append is never silent.
//!
//! A state change and a `pending_events` row commit in one transaction; the event is
//! then emitted through the [`EventSink`] and the pending row removed in a second
//! transaction that also advances the session's cursor. If the emit fails, the
//! pending row becomes an `emit_failed` [`EventGap`](crate::EventGap). If the process
//! dies between the two, startup recovery turns the leftover row into an
//! `interrupted` gap. Events are never re-emitted or synthesised later: replaying an
//! old state change with a new timestamp would invent history.

use aidex_core::{EntityId, Timestamp};
use aidex_events::{Cursor, EventKind, EventScope};
use rusqlite::{Connection, Transaction, params};

use crate::{
    EventGap, EventGapKind, Session, SessionError, SessionState, SessionStateKind, SessionStore,
    store::rows::cursor_sql,
};

/// An event to emit once the transaction that recorded it has committed.
pub(crate) struct Pending {
    id: i64,
    event: EventKind,
}

/// Record that `event` is about to be emitted for `session_id` (inside the change's
/// transaction).
pub(crate) fn stage(
    transaction: &Transaction<'_>,
    session_id: &EntityId,
    event: EventKind,
) -> Result<Pending, SessionError> {
    transaction.execute(
        "INSERT INTO pending_events (session_id, event_name, created_at) VALUES (?1, ?2, ?3)",
        params![session_id.as_str(), event.name(), Timestamp::now().as_str()],
    )?;
    Ok(Pending {
        id: transaction.last_insert_rowid(),
        event,
    })
}

impl SessionStore {
    /// Emit staged events in order and advance the session's cursor.
    ///
    /// The state change has already committed, so this never fails the operation: if
    /// the bookkeeping transaction itself fails, the pending row stays behind and the
    /// next startup reports it as an `interrupted` gap.
    pub(crate) fn publish(
        &self,
        connection: &mut Connection,
        session: &mut Session,
        pending: Vec<Pending>,
    ) {
        for Pending { id, event } in pending {
            let name = event.name();
            let scope = EventScope::session(session.workspace_id.clone(), session.id.clone());
            let outcome = self.events.emit(scope, event);
            let settled = settle(connection, session, id, name, outcome);
            if let Ok(Some(cursor)) = settled {
                session.last_event_cursor = cursor;
            }
        }
    }
}

/// Remove the pending row and either advance the cursor or record the failure.
fn settle(
    connection: &mut Connection,
    session: &Session,
    pending_id: i64,
    name: &str,
    outcome: Result<Cursor, aidex_events::EventError>,
) -> Result<Option<Cursor>, SessionError> {
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM pending_events WHERE id = ?1", [pending_id])?;
    let advanced = match outcome {
        Ok(cursor) if cursor > session.last_event_cursor => {
            transaction.execute(
                "UPDATE sessions SET last_event_cursor = ?2 WHERE id = ?1 AND last_event_cursor < ?2",
                params![session.id.as_str(), cursor_sql(cursor)],
            )?;
            Some(cursor)
        }
        Ok(_) => None,
        Err(error) => {
            record_gap(
                &transaction,
                &session.id,
                GapRecord {
                    kind: EventGapKind::EmitFailed,
                    event: Some(name),
                    detail: &format!("the event could not be recorded: {error}"),
                    session_cursor: Some(session.last_event_cursor),
                    log_cursor: None,
                },
            )?;
            None
        }
    };
    transaction.commit()?;
    Ok(advanced)
}

/// Fields of one `event_gaps` row.
pub(crate) struct GapRecord<'a> {
    pub kind: EventGapKind,
    pub event: Option<&'a str>,
    pub detail: &'a str,
    pub session_cursor: Option<Cursor>,
    pub log_cursor: Option<Cursor>,
}

pub(crate) fn record_gap(
    connection: &Connection,
    session_id: &EntityId,
    gap: GapRecord<'_>,
) -> Result<EventGap, SessionError> {
    let detected_at = Timestamp::now();
    connection.execute(
        "INSERT INTO event_gaps (session_id, kind, event_name, detail, session_cursor, log_cursor, detected_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            session_id.as_str(),
            gap.kind.as_str(),
            gap.event,
            gap.detail,
            gap.session_cursor.map(cursor_sql),
            gap.log_cursor.map(cursor_sql),
            detected_at.as_str(),
        ],
    )?;
    Ok(EventGap {
        session_id: session_id.clone(),
        kind: gap.kind,
        event: gap.event.map(str::to_owned),
        detail: gap.detail.to_owned(),
        session_cursor: gap.session_cursor,
        log_cursor: gap.log_cursor,
        detected_at,
    })
}

/// The `session.*` event for a state change, if the catalog has one.
///
/// Waiting for and returning from an approval are reported by the broker's
/// `approval.*` events in the same session scope, so they emit nothing here.
pub(crate) fn transition_event(from: SessionStateKind, to: &SessionState) -> Option<EventKind> {
    Some(match to {
        SessionState::Created => return None,
        SessionState::Running => match from {
            SessionStateKind::Created => EventKind::SessionStarted {},
            SessionStateKind::Paused | SessionStateKind::Interrupted => {
                EventKind::SessionResumed {}
            }
            _ => return None,
        },
        SessionState::WaitingApproval => return None,
        SessionState::Paused => EventKind::SessionPaused {},
        SessionState::Completed => EventKind::SessionCompleted {},
        SessionState::Failed { code, message } => EventKind::SessionFailed {
            reason: format!("{code}: {message}"),
        },
        SessionState::Cancelled => EventKind::SessionCancelled {},
        SessionState::Interrupted {
            reason,
            recoverable,
        } => EventKind::SessionInterrupted {
            reason: reason.clone(),
            recoverable: *recoverable,
        },
    })
}
