//! Versioned, transport-safe service contract types shared across AiDEX Rust
//! components. Secret material never belongs in this crate's wire models.

use std::{error::Error, fmt};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// An opaque, caller-assigned identifier safe to use as a logical key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdError {
    Empty,
    TooLong,
    InvalidCharacter(char),
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("identifier must not be empty"),
            Self::TooLong => f.write_str("identifier must be at most 128 bytes"),
            Self::InvalidCharacter(ch) => write!(f, "invalid identifier character: {ch:?}"),
        }
    }
}

impl Error for IdError {}

impl EntityId {
    /// Construct an ID from ASCII letters, digits, `-`, `_`, `.`, or `:`.
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(IdError::Empty);
        }
        if value.len() > 128 {
            return Err(IdError::TooLong);
        }
        for ch in value.chars() {
            if !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':')) {
                return Err(IdError::InvalidCharacter(ch));
            }
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for EntityId {
    type Error = IdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for EntityId {
    type Error = IdError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl Serialize for EntityId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EntityId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl JsonSchema for EntityId {
    fn schema_name() -> String {
        "EntityId".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}

/// Validated RFC 3339 UTC timestamp, serialized canonically with a trailing `Z`.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Timestamp(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimestampError;

impl fmt::Display for TimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("timestamp must be a valid RFC 3339 UTC timestamp ending in Z")
    }
}
impl Error for TimestampError {}

impl Timestamp {
    pub fn new(value: impl Into<String>) -> Result<Self, TimestampError> {
        let value = value.into();
        if !valid_timestamp(&value) {
            return Err(TimestampError);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || *b.last().unwrap_or(&0) != b'Z'
        || ![0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
    {
        return false;
    }
    let num = |start: usize, end: usize| s[start..end].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        num(0, 4),
        num(5, 7),
        num(8, 10),
        num(11, 13),
        num(14, 16),
        num(17, 19),
    ) else {
        return false;
    };
    if !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > days {
        return false;
    }
    if b.len() == 20 {
        return true;
    }
    if b[19] != b'.' || b.len() < 22 || b.len() > 30 {
        return false;
    }
    b[20..b.len() - 1].iter().all(u8::is_ascii_digit)
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(de::Error::custom)
    }
}
impl JsonSchema for Timestamp {
    fn schema_name() -> String {
        "Timestamp".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}

/// A normalized relative path using `/` separators, never an absolute path.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WorkspacePath(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspacePathError;

impl fmt::Display for WorkspacePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("path must be a non-empty workspace-relative path without traversal")
    }
}
impl Error for WorkspacePathError {}

impl WorkspacePath {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspacePathError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 4096
            || value.starts_with('/')
            || value.contains('\\')
            || value.contains(':')
            || value.chars().any(char::is_control)
            || value
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(WorkspacePathError);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Serialize for WorkspacePath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for WorkspacePath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(de::Error::custom)
    }
}
impl JsonSchema for WorkspacePath {
    fn schema_name() -> String {
        "WorkspacePath".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}

/// Reference to a secret held by a host vault. Secret bytes/values are never serialized.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecretRef {
    pub vault_id: EntityId,
    pub secret_id: EntityId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ContractVersion(pub u16);

impl ContractVersion {
    pub const CURRENT: Self = Self(1);
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Versioned<T> {
    pub version: ContractVersion,
    pub payload: T,
}

impl<T> Versioned<T> {
    pub fn new(payload: T) -> Self {
        Self {
            version: ContractVersion::CURRENT,
            payload,
        }
    }
    pub fn validate_version(&self) -> Result<(), ServiceErrorCode> {
        if self.version == ContractVersion::CURRENT {
            Ok(())
        } else {
            Err(ServiceErrorCode::IncompatibleVersion)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorCode {
    InvalidArgument,
    NotFound,
    PermissionDenied,
    Conflict,
    IncompatibleVersion,
    Unavailable,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceError {
    pub code: ServiceErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<EntityId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContractOffer {
    pub minimum: ContractVersion,
    pub maximum: ContractVersion,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_v1_schema_matches_checked_in_client_boundary() {
        let generated = format!(
            "{}\n",
            serde_json::to_string_pretty(&schemars::schema_for!(Versioned<ContractPayload>))
                .unwrap()
        );
        assert_eq!(
            generated,
            include_str!("../../../protocol/schemas/contract-v1.schema.json")
        );
    }

    #[test]
    fn validated_types_reject_malformed_wire_values() {
        assert!(serde_json::from_str::<EntityId>("\"bad/id\"").is_err());
        assert!(serde_json::from_str::<WorkspacePath>("\"../outside\"").is_err());
        assert!(serde_json::from_str::<WorkspacePath>("\"C:/outside\"").is_err());
        assert!(serde_json::from_str::<WorkspacePath>("\"line\\nbreak\"").is_err());
        assert!(serde_json::from_str::<Timestamp>("\"2024-02-30T12:00:00Z\"").is_err());
        assert!(serde_json::from_str::<Timestamp>("\"not-a-time\"").is_err());
    }

    #[test]
    fn versioned_requests_and_events_round_trip_and_check_version() {
        let request = Versioned::new(ToolIntentRequest {
            session_id: EntityId::new("session-1").unwrap(),
            tool_id: EntityId::new("shell").unwrap(),
            input: serde_json::json!({"command":"pwd"}),
            requested_at: Timestamp::new("2026-10-10T07:51:01Z").unwrap(),
        });
        let json = serde_json::to_string(&request).unwrap();
        let decoded: Versioned<ToolIntentRequest> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
        assert_eq!(decoded.validate_version(), Ok(()));
        let incompatible = serde_json::from_str::<Versioned<ToolIntentRequest>>(
            r#"{"version":2,"payload":{"session_id":"session-1","tool_id":"shell","input":{},"requested_at":"2026-10-10T07:51:01Z"}}"#,
        ).unwrap();
        assert_eq!(
            incompatible.validate_version(),
            Err(ServiceErrorCode::IncompatibleVersion)
        );
        let event = ContractEvent::ApprovalRecorded {
            approval_id: EntityId::new("approval-1").unwrap(),
            intent_id: EntityId::new("intent-1").unwrap(),
            decision: ApprovalDecision::Approve,
            occurred_at: Timestamp::new("2026-10-10T07:51:01Z").unwrap(),
        };
        assert_eq!(
            serde_json::from_str::<ContractEvent>(&serde_json::to_string(&event).unwrap()).unwrap(),
            event
        );
    }

    #[test]
    fn secret_contract_contains_references_only() {
        let secret = SecretRef {
            vault_id: EntityId::new("vault-1").unwrap(),
            secret_id: EntityId::new("token").unwrap(),
        };
        let json = serde_json::to_value(secret).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"vault_id":"vault-1","secret_id":"token"})
        );
        assert!(
            serde_json::from_str::<SecretRef>(
                r#"{"vault_id":"v","secret_id":"s","value":"raw-secret"}"#
            )
            .is_err()
        );
    }
}
