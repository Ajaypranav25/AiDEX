//! The session record, its explicit state machine, and request types.

use std::{fmt, str::FromStr};

use aidex_core::{EntityId, ModelReference, Timestamp};
use aidex_events::Cursor;
use serde::{Deserialize, Serialize};

use crate::{HostFingerprint, SessionError};

/// Identifier of a checkpoint (`chk_…`).
pub type CheckpointId = EntityId;

/// Longest session title, in bytes.
pub const MAX_TITLE_BYTES: usize = 512;
/// Longest failure message or interruption reason, in bytes.
pub const MAX_REASON_BYTES: usize = 4 * 1024;
/// Longest failure code, in bytes (`[a-z0-9_.-]`).
pub const MAX_CODE_BYTES: usize = 64;
/// Most sessions returned by one [`crate::SessionStore::list`] call.
pub const MAX_LIST_LIMIT: usize = 500;
/// Page size used when a [`SessionFilter`] names none.
pub const DEFAULT_LIST_LIMIT: usize = 50;

/// A durable agent session. Records hold ids and drive-relative data only, never
/// absolute host paths, so they survive a daemon restart and a drive move.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    /// `ses_<ULID>`; stable for the life of the session.
    pub id: EntityId,
    pub workspace_id: EntityId,
    pub agent_id: EntityId,
    pub title: String,
    pub state: SessionState,
    /// Parent of a subagent session or of a branch.
    pub parent_session_id: Option<EntityId>,
    /// Checkpoint of the parent this branch was seeded from (spec §74).
    pub branched_from: Option<CheckpointId>,
    pub model: Option<ModelReference>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// The host this session last ran on (or was created/resumed on).
    pub host_fingerprint: HostFingerprint,
    /// High-water mark: every event of this session has a cursor at or below it.
    pub last_event_cursor: Cursor,
    pub budget: BudgetUsage,
    /// When the session was last adopted for resumption ([`crate::SessionStore::mark_resumed`]).
    pub resumed_at: Option<Timestamp>,
}

impl Session {
    pub fn kind(&self) -> SessionStateKind {
        self.state.kind()
    }
}

/// Accumulated resource use of a session. Additions saturate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u64,
    pub wall_ms: u64,
}

impl BudgetUsage {
    #[must_use]
    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens.saturating_add(other.input_tokens),
            output_tokens: self.output_tokens.saturating_add(other.output_tokens),
            tool_calls: self.tool_calls.saturating_add(other.tool_calls),
            wall_ms: self.wall_ms.saturating_add(other.wall_ms),
        }
    }
}

/// Lifecycle state. Wire form: `{"kind": "interrupted", "reason": "...", "recoverable": true}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionState {
    Created,
    Running,
    WaitingApproval,
    Paused,
    Completed,
    Failed {
        code: String,
        message: String,
    },
    Cancelled,
    /// Stopped without a clean transition (crash, restart, drive removal).
    /// `recoverable` means a verified checkpoint existed when it was interrupted.
    Interrupted {
        reason: String,
        recoverable: bool,
    },
}

impl SessionState {
    pub fn kind(&self) -> SessionStateKind {
        match self {
            Self::Created => SessionStateKind::Created,
            Self::Running => SessionStateKind::Running,
            Self::WaitingApproval => SessionStateKind::WaitingApproval,
            Self::Paused => SessionStateKind::Paused,
            Self::Completed => SessionStateKind::Completed,
            Self::Failed { .. } => SessionStateKind::Failed,
            Self::Cancelled => SessionStateKind::Cancelled,
            Self::Interrupted { .. } => SessionStateKind::Interrupted,
        }
    }

    pub fn is_terminal(&self) -> bool {
        self.kind().is_terminal()
    }
}

/// The state name without its details; used for filters and the transition table.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStateKind {
    Created,
    Running,
    WaitingApproval,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl SessionStateKind {
    pub const ALL: [Self; 8] = [
        Self::Created,
        Self::Running,
        Self::WaitingApproval,
        Self::Paused,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
        Self::Interrupted,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    /// Completed, failed, and cancelled sessions never change state again.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Sessions that were doing work when the daemon stopped.
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::WaitingApproval)
    }

    /// The transition table. Same-state moves are not transitions.
    ///
    /// | from | allowed to |
    /// |---|---|
    /// | created | running, failed, cancelled |
    /// | running | waiting_approval, paused, completed, failed, cancelled, interrupted |
    /// | waiting_approval | running, paused, failed, cancelled, interrupted |
    /// | paused | running, failed, cancelled |
    /// | interrupted | running, paused (adopted for resume), failed, cancelled |
    /// | completed, failed, cancelled | — (terminal) |
    pub const fn can_transition_to(self, next: Self) -> bool {
        use SessionStateKind::{
            Cancelled, Completed, Created, Failed, Interrupted, Paused, Running, WaitingApproval,
        };
        matches!(
            (self, next),
            (Created, Running | Failed | Cancelled)
                | (
                    Running,
                    WaitingApproval | Paused | Completed | Failed | Cancelled | Interrupted
                )
                | (
                    WaitingApproval,
                    Running | Paused | Failed | Cancelled | Interrupted
                )
                | (Paused, Running | Failed | Cancelled)
                | (Interrupted, Running | Paused | Failed | Cancelled)
        )
    }
}

impl fmt::Display for SessionStateKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SessionStateKind {
    type Err = SessionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| {
                SessionError::InvalidArgument(format!(
                    "unknown session state `{value}` (expected one of: {})",
                    Self::ALL.map(Self::as_str).join(", ")
                ))
            })
    }
}

/// Input of [`crate::SessionStore::create`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewSession {
    pub workspace_id: EntityId,
    pub agent_id: EntityId,
    pub title: String,
    /// Parent of a subagent session.
    pub parent_session_id: Option<EntityId>,
    pub model: Option<ModelReference>,
}

impl NewSession {
    pub fn new(workspace_id: EntityId, agent_id: EntityId, title: impl Into<String>) -> Self {
        Self {
            workspace_id,
            agent_id,
            title: title.into(),
            parent_session_id: None,
            model: None,
        }
    }

    #[must_use]
    pub fn with_parent(mut self, parent: EntityId) -> Self {
        self.parent_session_id = Some(parent);
        self
    }

    #[must_use]
    pub fn with_model(mut self, model: ModelReference) -> Self {
        self.model = Some(model);
        self
    }
}

/// Query of [`crate::SessionStore::list`]; newest sessions first.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<SessionStateKind>,
    /// 1 to [`MAX_LIST_LIMIT`]; [`DEFAULT_LIST_LIMIT`] when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl SessionFilter {
    pub fn all() -> Self {
        Self::default()
    }

    pub fn for_workspace(workspace_id: EntityId) -> Self {
        Self {
            workspace_id: Some(workspace_id),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_state(mut self, state: SessionStateKind) -> Self {
        self.state = Some(state);
        self
    }

    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub(crate) fn effective_limit(&self) -> Result<usize, SessionError> {
        match self.limit {
            None => Ok(DEFAULT_LIST_LIMIT),
            Some(limit) if (1..=MAX_LIST_LIMIT).contains(&limit) => Ok(limit),
            Some(limit) => Err(SessionError::InvalidArgument(format!(
                "limit {limit} must be between 1 and {MAX_LIST_LIMIT}"
            ))),
        }
    }
}

/// Reject over-long text; returns the input for chaining.
pub(crate) fn bounded<'a>(
    what: &str,
    text: &'a str,
    max: usize,
    allow_empty: bool,
) -> Result<&'a str, SessionError> {
    if !allow_empty && text.trim().is_empty() {
        return Err(SessionError::InvalidArgument(format!(
            "{what} must not be empty"
        )));
    }
    if text.len() > max {
        return Err(SessionError::InvalidArgument(format!(
            "{what} is {} bytes; the limit is {max}",
            text.len()
        )));
    }
    Ok(text)
}

/// Validate a stable machine code such as a failure code.
pub(crate) fn validate_code(what: &str, code: &str) -> Result<(), SessionError> {
    if code.is_empty()
        || code.len() > MAX_CODE_BYTES
        || !code.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
    {
        return Err(SessionError::InvalidArgument(format!(
            "{what} must be 1-{MAX_CODE_BYTES} characters of [a-z0-9_.-]"
        )));
    }
    Ok(())
}
