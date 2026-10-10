//! Request, response, reference, and event DTOs exposed by the version-1 contract.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractOffer, EntityId, ServiceError, Timestamp, WorkspacePath};

/// Reference to a secret held by a host vault. Secret bytes/values are never serialized.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecretRef {
    pub vault_id: EntityId,
    pub secret_id: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRequest {
    pub workspace_id: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceResponse {
    pub workspace_id: EntityId,
    pub root: WorkspacePath,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionRequest {
    pub workspace_id: EntityId,
    pub agent_id: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionResponse {
    pub session_id: EntityId,
    pub started_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentRequest {
    pub agent_id: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentResponse {
    pub agent_id: EntityId,
    pub model: ModelReference,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelReference {
    pub provider_id: EntityId,
    pub model_id: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolIntentRequest {
    pub session_id: EntityId,
    pub tool_id: EntityId,
    pub input: serde_json::Value,
    pub requested_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolIntentResponse {
    pub intent_id: EntityId,
    pub state: ToolIntentState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolIntentState {
    PendingApproval,
    Approved,
    Denied,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRequest {
    pub intent_id: EntityId,
    pub decision: ApprovalDecision,
    pub decided_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approve,
    Deny,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalResponse {
    pub approval_id: EntityId,
    pub decision: ApprovalDecision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostReference {
    pub host_id: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VaultReference {
    pub vault_id: EntityId,
    pub host_id: EntityId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContractEvent {
    ToolIntentChanged {
        intent_id: EntityId,
        state: ToolIntentState,
        occurred_at: Timestamp,
    },
    ApprovalRecorded {
        approval_id: EntityId,
        intent_id: EntityId,
        decision: ApprovalDecision,
        occurred_at: Timestamp,
    },
    WorkspaceChanged {
        workspace_id: EntityId,
        path: WorkspacePath,
        occurred_at: Timestamp,
    },
}

/// All payload shapes exposed by the version-1 contract schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ContractPayload {
    ContractOffer(ContractOffer),
    WorkspaceRequest(WorkspaceRequest),
    WorkspaceResponse(WorkspaceResponse),
    SessionRequest(SessionRequest),
    SessionResponse(SessionResponse),
    AgentRequest(AgentRequest),
    AgentResponse(AgentResponse),
    ModelReference(ModelReference),
    ToolIntentRequest(ToolIntentRequest),
    ToolIntentResponse(ToolIntentResponse),
    ApprovalRequest(ApprovalRequest),
    ApprovalResponse(ApprovalResponse),
    HostReference(HostReference),
    VaultReference(VaultReference),
    SecretRef(SecretRef),
    ServiceError(ServiceError),
    Event(ContractEvent),
}
