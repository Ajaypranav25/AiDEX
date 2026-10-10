//! Versioned, transport-safe service contract types shared across AiDEX Rust
//! components. Secret material never belongs in this crate's wire models.

mod cancel;
mod contract;
mod error;
mod id;
mod path;
mod time;
mod version;

pub use cancel::{CancelReason, CancelToken};
pub use contract::{
    AgentRequest, AgentResponse, ApprovalDecision, ApprovalRequest, ApprovalResponse,
    ContractEvent, ContractPayload, HostReference, ModelReference, SecretRef, SessionRequest,
    SessionResponse, ToolIntentRequest, ToolIntentResponse, ToolIntentState, VaultReference,
    WorkspaceRequest, WorkspaceResponse,
};
pub use error::{ServiceError, ServiceErrorCode};
pub use id::{EntityId, IdError};
pub use path::{WorkspacePath, WorkspacePathError};
pub use time::{Timestamp, TimestampError};
pub use version::{
    ContractNegotiationError, ContractOffer, ContractVersion, ContractVersionError, Versioned,
};

#[cfg(test)]
mod tests;
