//! Resume plans for continuing a session, possibly on another host (spec §22, §39).
//!
//! The plan is what the harness (#19) needs to continue: the newest verified
//! checkpoint, the messages after it, and an explicit list of everything that did
//! not survive (processes, unfinished tool calls, approvals, model state). The store
//! never restarts work itself; [`SessionStore::mark_resumed`] only adopts the
//! session on this host and leaves it `paused` for the harness to run.

use aidex_core::{EntityId, Timestamp};
use serde::{Deserialize, Serialize};

use crate::{
    Checkpoint, CheckpointId, EventGap, HostFingerprint, Message, Session, SessionError,
    SessionState, SessionStateKind, SessionStore, ToolCallState,
    approvals::{ApprovalStatus, load_approvals},
    checkpoints::load_metas,
    messages::load_messages,
    recovery::load_gaps,
    store::rows::load_session,
    tool_calls::load_tool_calls,
};

/// What did not survive and must be recreated or redone.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnrecoverableKind {
    /// A host process (never serialized; recreated if needed).
    Process,
    /// A tool call whose outcome is unknown or that never ran.
    ToolCall,
    /// An approval that is not carried over; the broker must ask again.
    Approval,
    /// In-memory model state (KV cache, sampling position, loaded runtime).
    ModelState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnrecoverableItem {
    pub kind: UnrecoverableKind,
    /// The process, tool call, approval, or model the item is about.
    pub subject: Option<EntityId>,
    pub detail: String,
}

/// Everything needed to decide on and perform a resume.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumePlan {
    pub session: Session,
    /// The newest verified checkpoint; `None` means resume from messages only.
    pub checkpoint: Option<Checkpoint>,
    /// The newest checkpoint was corrupt and an older one was chosen.
    pub recovered_from_corruption: bool,
    /// Every checkpoint of the session that failed verification, oldest first.
    pub corrupt_checkpoints: Vec<CheckpointId>,
    /// Messages after the checkpoint (all messages without one), ascending.
    pub messages_since_checkpoint: Vec<Message>,
    pub unrecoverable: Vec<UnrecoverableItem>,
    /// The session last ran on a different host than `current_host`.
    pub different_host: bool,
    /// Human-readable host differences (empty on the same host).
    pub host_changes: Vec<String>,
    /// Recorded holes in the session's event history.
    pub event_gaps: Vec<EventGap>,
    /// The session is `interrupted` or `paused` and may be continued.
    pub resumable: bool,
    /// Why it may not be continued, when it may not.
    pub blocked_reason: Option<String>,
}

impl SessionStore {
    /// Build the resume plan for `id` as seen from `current_host`. Read-only except
    /// that checkpoints failing verification are marked corrupt.
    pub fn resume_plan(
        &self,
        id: &EntityId,
        current_host: &HostFingerprint,
    ) -> Result<ResumePlan, SessionError> {
        let connection = self.lock();
        let session = load_session(&connection, id)?;
        let restored = match self.restore_latest(&connection, id) {
            Ok(restored) => restored,
            Err(SessionError::NoValidCheckpoint { .. }) => None,
            Err(error) => return Err(error),
        };
        let corrupt_checkpoints = load_metas(&connection, id)?
            .into_iter()
            .filter(|meta| !meta.is_valid())
            .map(|meta| meta.id)
            .collect();
        let (checkpoint, recovered_from_corruption) = match restored {
            Some(restored) => (
                Some(restored.checkpoint),
                restored.recovered_from_corruption,
            ),
            None => (None, false),
        };
        let after = checkpoint
            .as_ref()
            .map_or(0, |checkpoint| checkpoint.marks.message_seq);
        let messages_since_checkpoint = load_messages(&connection, id, after, None)?;
        let different_host = session.host_fingerprint.id != current_host.id;
        let host_changes = current_host.changes_from(&session.host_fingerprint);

        let mut unrecoverable = Vec::new();
        for call in load_tool_calls(&connection, id)? {
            match call.state {
                ToolCallState::Lost => {
                    if let Some(process) = &call.process_id {
                        unrecoverable.push(UnrecoverableItem {
                            kind: UnrecoverableKind::Process,
                            subject: Some(process.clone()),
                            detail: format!(
                                "process {process} of tool call {} ran under the previous daemon and was not restored; host processes are recreated, never serialized",
                                call.call_id
                            ),
                        });
                    }
                    unrecoverable.push(UnrecoverableItem {
                        kind: UnrecoverableKind::ToolCall,
                        subject: Some(call.call_id.clone()),
                        detail: format!(
                            "tool call {} ({}) was interrupted and its outcome is unknown; verify its effects or run it again",
                            call.call_id, call.tool_id
                        ),
                    });
                }
                ToolCallState::Requested | ToolCallState::AwaitingApproval
                    if !session.kind().is_active() =>
                {
                    unrecoverable.push(UnrecoverableItem {
                        kind: UnrecoverableKind::ToolCall,
                        subject: Some(call.call_id.clone()),
                        detail: format!(
                            "tool call {} ({}) never ran; it must be requested (and approved) again",
                            call.call_id, call.tool_id
                        ),
                    });
                }
                _ => {}
            }
        }
        let carried = checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.marks.open_approvals.clone())
            .unwrap_or_default();
        for record in load_approvals(&connection, id)? {
            let approval = &record.approval;
            let pending = approval.decision == ApprovalStatus::Pending;
            if pending || carried.contains(&approval.approval_id) {
                unrecoverable.push(UnrecoverableItem {
                    kind: UnrecoverableKind::Approval,
                    subject: Some(approval.approval_id.clone()),
                    detail: format!(
                        "approval {} ({}) is not carried over; the broker must ask again",
                        approval.approval_id,
                        approval.decision.as_str()
                    ),
                });
            }
        }
        unrecoverable.push(UnrecoverableItem {
            kind: UnrecoverableKind::ModelState,
            subject: None,
            detail: "in-memory model state (KV cache, sampling position, loaded runtime) is not persisted; the model context is rebuilt from the checkpoint and messages".into(),
        });
        if let (Some(model), true) = (&session.model, different_host) {
            unrecoverable.push(UnrecoverableItem {
                kind: UnrecoverableKind::ModelState,
                subject: Some(model.model_id.clone()),
                detail: format!(
                    "model {}/{} was selected on another host; confirm it is available here or let the router choose again",
                    model.provider_id, model.model_id
                ),
            });
        }
        let blocked_reason = match session.kind() {
            SessionStateKind::Interrupted | SessionStateKind::Paused => None,
            SessionStateKind::Running | SessionStateKind::WaitingApproval => {
                Some("the session is still active on this daemon".to_owned())
            }
            SessionStateKind::Created => {
                Some("the session has not started; start it instead of resuming".to_owned())
            }
            terminal => Some(format!("the session is {terminal}")),
        };
        Ok(ResumePlan {
            event_gaps: load_gaps(&connection, id)?,
            resumable: blocked_reason.is_none(),
            blocked_reason,
            session,
            checkpoint,
            recovered_from_corruption,
            corrupt_checkpoints,
            messages_since_checkpoint,
            unrecoverable,
            different_host,
            host_changes,
        })
    }

    /// Adopt an `interrupted` or `paused` session on this host for resumption.
    ///
    /// An interrupted session becomes `paused` (`session.paused`); both record this
    /// host as the session's host and set `resumed_at`. Nothing is started: the
    /// harness continues the session by moving it to `running` (`session.resumed`).
    pub fn mark_resumed(&self, id: &EntityId) -> Result<Session, SessionError> {
        let mut connection = self.lock();
        let session = load_session(&connection, id)?;
        let now = Timestamp::now();
        match session.kind() {
            SessionStateKind::Interrupted => {
                self.transition_locked(&mut connection, id, SessionState::Paused, Some(&now))
            }
            SessionStateKind::Paused => {
                connection.execute(
                    "UPDATE sessions SET host_json = ?2, resumed_at = ?3, updated_at = ?3 WHERE id = ?1",
                    rusqlite::params![
                        id.as_str(),
                        crate::store::rows::encode(self.host())?,
                        now.as_str()
                    ],
                )?;
                load_session(&connection, id)
            }
            from => Err(SessionError::InvalidTransition {
                from,
                to: SessionStateKind::Paused,
            }),
        }
    }
}
