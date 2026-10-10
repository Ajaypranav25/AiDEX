//! `aidex-sessions`: durable agent sessions on the drive (spec §20-22, §74, §77, §87).
//!
//! - [`SessionStore`]: SQLite (`<private_sessions>/sessions.sqlite3`) plus a
//!   content-addressed blob store for sessions, messages, tool calls, approval
//!   references, artifacts, and checkpoints, with an explicit state machine whose
//!   transitions emit `session.*` events through an [`aidex_events::EventSink`].
//! - Checkpoints ([`SessionStore::checkpoint`]) are hash-verified on restore; a
//!   corrupt latest checkpoint falls back to the previous valid one.
//! - [`SessionStore::recover_on_startup`] turns an unclean stop into explicit
//!   interrupted sessions, lost tool calls, and recorded event gaps.
//! - [`SessionStore::resume_plan`] reports what a resume on this host restores and
//!   what it cannot ([`UnrecoverableItem`]); continuing is the harness's job.
//! - [`HostFingerprint`]: salted, privacy-preserving host identity (spec §62).
//!
//! Records never hold secret values or absolute host paths: credentials are vault
//! handles, and every persisted text field is redacted as defence in depth.

mod approvals;
mod artifacts;
mod checkpoints;
mod error;
mod host;
mod messages;
mod migration;
mod model;
mod recovery;
mod resume;
mod secrets;
mod store;
mod summary;
mod tool_calls;

pub use aidex_events::MessageRole;
pub use approvals::{ApprovalRecord, ApprovalRef, ApprovalScope, ApprovalStatus};
pub use artifacts::{ArtifactKind, ArtifactRef, MAX_ARTIFACT_BYTES, MAX_SOURCE_PATHS, NewArtifact};
pub use checkpoints::{
    CHECKPOINT_FORMAT, Checkpoint, CheckpointMeta, CheckpointStatus, HighWaterMarks, LogicalState,
    MAX_CHECKPOINT_BYTES, MAX_LABEL_BYTES, RestoredCheckpoint, ToolCallMark,
};
pub use error::SessionError;
pub use host::{
    FINGERPRINT_HEX_LEN, HOST_SALT_BYTES, HOST_SALT_FILE, HostFingerprint, load_or_create_host_salt,
};
pub use messages::{MAX_MESSAGE_BYTES, MAX_MESSAGE_PAGE, Message, MessagePage, NewMessage};
pub use migration::SCHEMA_VERSION;
pub use model::{
    BudgetUsage, CheckpointId, DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT, MAX_REASON_BYTES,
    MAX_TITLE_BYTES, NewSession, Session, SessionFilter, SessionState, SessionStateKind,
};
pub use recovery::{
    CancelledApproval, EventGap, EventGapKind, InterruptedSession, LostToolCall, RESTART_REASON,
    RecoveryReport,
};
pub use resume::{ResumePlan, UnrecoverableItem, UnrecoverableKind};
pub use secrets::{REDACTED, contains_secret_like};
pub use store::{BLOB_DIR, SESSIONS_FILE, SessionStore};
pub use summary::SessionSummary;
pub use tool_calls::{
    MAX_SUMMARY_BYTES, MAX_TOOL_INPUT_BYTES, NewToolCall, ToolCallRecord, ToolCallState,
    ToolCallUpdate,
};
