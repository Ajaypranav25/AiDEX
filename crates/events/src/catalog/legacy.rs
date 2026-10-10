//! Import of PR #40 (event log v1) records into the current catalog.
//!
//! v1 rows store `{"version":1,"sequence":N,"workspace_id":"..","session_id":..,
//! "kind":{"type":"task_started","task_id":".."}}`. Records with a faithful current
//! equivalent map onto it; the rest become [`EventKind::LegacyV1`], which preserves
//! the original typed record instead of inventing fields v1 never captured.

use aidex_core::EntityId;
use serde::{Deserialize, Serialize};

use super::{EventKind, Outcome};
use crate::{EventError, EventScope};

/// The PR #40 transition a `legacy.v1` event preserves.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyRecord {
    TaskStarted,
    TaskFinished,
    SessionStarted,
    SessionFinished,
    AgentStarted,
    AgentFinished,
    ToolStarted,
    ToolFinished,
    ApprovalRequested,
    ApprovalResolved,
    ProcessStarted,
    ProcessFinished,
    ModelStarted,
    ModelFinished,
    CleanupStarted,
    CleanupFinished,
}

/// PR #40's approval resolution.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyApprovalDecision {
    Approved,
    Denied,
    Expired,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V1Event {
    version: u32,
    #[allow(dead_code)]
    sequence: u64,
    workspace_id: String,
    session_id: Option<String>,
    kind: V1Kind,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum V1Kind {
    TaskStarted {
        task_id: String,
    },
    TaskFinished {
        task_id: String,
        outcome: Outcome,
    },
    SessionStarted,
    SessionFinished,
    AgentStarted {
        agent_id: String,
    },
    AgentFinished {
        agent_id: String,
        outcome: Outcome,
    },
    ToolStarted {
        tool_id: String,
    },
    ToolFinished {
        tool_id: String,
        outcome: Outcome,
    },
    ApprovalRequested {
        approval_id: String,
    },
    ApprovalResolved {
        approval_id: String,
        decision: LegacyApprovalDecision,
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
        outcome: Outcome,
    },
    CleanupStarted {
        cleanup_id: String,
    },
    CleanupFinished {
        cleanup_id: String,
        outcome: Outcome,
    },
}

fn id(value: String) -> Result<EntityId, EventError> {
    EntityId::new(value)
        .map_err(|error| EventError::Migration(format!("invalid v1 identifier: {error}")))
}

fn legacy(record: LegacyRecord, subject: Option<String>) -> Result<EventKind, EventError> {
    Ok(EventKind::LegacyV1 {
        record,
        subject_id: subject.map(id).transpose()?,
        outcome: None,
        decision: None,
        exit_code: None,
    })
}

fn legacy_outcome(
    record: LegacyRecord,
    subject: String,
    outcome: Outcome,
) -> Result<EventKind, EventError> {
    Ok(EventKind::LegacyV1 {
        record,
        subject_id: Some(id(subject)?),
        outcome: Some(outcome),
        decision: None,
        exit_code: None,
    })
}

/// Decode one stored v1 body into its scope and current-catalog event.
pub(crate) fn import_v1_body(body: &str) -> Result<(EventScope, EventKind), EventError> {
    let event: V1Event = serde_json::from_str(body)
        .map_err(|error| EventError::Migration(format!("unreadable v1 event: {error}")))?;
    if event.version != 1 {
        return Err(EventError::Migration(format!(
            "unexpected v1 event version {}",
            event.version
        )));
    }
    let scope = EventScope {
        workspace_id: Some(id(event.workspace_id)?),
        session_id: event.session_id.map(id).transpose()?,
    };
    let kind = match event.kind {
        V1Kind::TaskStarted { task_id } => EventKind::TaskStarted {
            task_id: id(task_id)?,
        },
        V1Kind::TaskFinished {
            task_id,
            outcome: Outcome::Succeeded,
        } => EventKind::TaskCompleted {
            task_id: id(task_id)?,
        },
        V1Kind::TaskFinished {
            task_id,
            outcome: Outcome::Cancelled,
        } => EventKind::TaskCancelled {
            task_id: id(task_id)?,
        },
        // v1 recorded no failure reason, which `task.failed` requires.
        V1Kind::TaskFinished {
            task_id,
            outcome: Outcome::Failed,
        } => legacy_outcome(LegacyRecord::TaskFinished, task_id, Outcome::Failed)?,
        V1Kind::SessionStarted => EventKind::SessionStarted {},
        // "finished" did not say whether the session completed, failed, or was cancelled.
        V1Kind::SessionFinished => legacy(LegacyRecord::SessionFinished, None)?,
        V1Kind::AgentStarted { agent_id } => EventKind::AgentStarted {
            agent_id: id(agent_id)?,
            model: None,
        },
        V1Kind::AgentFinished { agent_id, outcome } => EventKind::AgentFinished {
            agent_id: id(agent_id)?,
            outcome,
        },
        // v1 `tool_id` was not a call id, and v1 had no duration, summary, or error.
        V1Kind::ToolStarted { tool_id } => legacy(LegacyRecord::ToolStarted, Some(tool_id))?,
        V1Kind::ToolFinished { tool_id, outcome } => {
            legacy_outcome(LegacyRecord::ToolFinished, tool_id, outcome)?
        }
        // v1 approvals had no intent, summary, or scope.
        V1Kind::ApprovalRequested { approval_id } => {
            legacy(LegacyRecord::ApprovalRequested, Some(approval_id))?
        }
        V1Kind::ApprovalResolved {
            approval_id,
            decision: LegacyApprovalDecision::Denied,
        } => EventKind::ApprovalDenied {
            approval_id: id(approval_id)?,
            reason: None,
        },
        V1Kind::ApprovalResolved {
            approval_id,
            decision: LegacyApprovalDecision::Expired,
        } => EventKind::ApprovalExpired {
            approval_id: id(approval_id)?,
        },
        V1Kind::ApprovalResolved {
            approval_id,
            decision: decision @ LegacyApprovalDecision::Approved,
        } => EventKind::LegacyV1 {
            record: LegacyRecord::ApprovalResolved,
            subject_id: Some(id(approval_id)?),
            outcome: None,
            decision: Some(decision),
            exit_code: None,
        },
        // v1 recorded neither the command nor the sandbox level.
        V1Kind::ProcessStarted { process_id } => {
            legacy(LegacyRecord::ProcessStarted, Some(process_id))?
        }
        V1Kind::ProcessFinished {
            process_id,
            exit_code,
        } => EventKind::ProcessExited {
            process_id: id(process_id)?,
            exit_code,
            signal: None,
        },
        // v1 model transitions named neither a provider nor whether a load happened.
        V1Kind::ModelStarted { model_id } => legacy(LegacyRecord::ModelStarted, Some(model_id))?,
        V1Kind::ModelFinished { model_id, outcome } => {
            legacy_outcome(LegacyRecord::ModelFinished, model_id, outcome)?
        }
        V1Kind::CleanupStarted { cleanup_id } => EventKind::CleanupStarted {
            cleanup_id: id(cleanup_id)?,
        },
        // v1 recorded no cleaned/failed/unmanaged counts.
        V1Kind::CleanupFinished {
            cleanup_id,
            outcome,
        } => legacy_outcome(LegacyRecord::CleanupFinished, cleanup_id, outcome)?,
    };
    Ok((scope, kind))
}
