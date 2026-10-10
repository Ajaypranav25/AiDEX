//! Tool identity, descriptors, risk metadata, and the model-facing tool spec.

use std::fmt;

use aidex_core::EntityId;
use aidex_events::RiskLevel;
use aidex_policy::Action;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;

use crate::{ToolError, schema};

/// Longest tool id in bytes.
const MAX_TOOL_ID_LEN: usize = 64;

/// A stable dotted tool identifier such as `fs.read` or `git.status`.
///
/// Two or more segments separated by `.`; each segment starts with a lowercase ASCII
/// letter followed by lowercase letters, digits, `_`, or `-`; at most 64 bytes. Every
/// tool id is also a valid [`EntityId`] (events and policy intents carry it as one).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ToolId(EntityId);

impl ToolId {
    pub fn new(id: impl Into<String>) -> Result<Self, ToolError> {
        let id = id.into();
        let valid_segment = |segment: &str| {
            let mut chars = segment.chars();
            chars.next().is_some_and(|first| first.is_ascii_lowercase())
                && chars.all(|ch| {
                    ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-'
                })
        };
        if id.len() > MAX_TOOL_ID_LEN || !id.contains('.') || !id.split('.').all(valid_segment) {
            return Err(ToolError::Registration(format!(
                "`{id}` is not a valid tool id (dotted lowercase segments, at most {MAX_TOOL_ID_LEN} bytes)"
            )));
        }
        EntityId::new(id)
            .map(Self)
            .map_err(|error| ToolError::Registration(format!("invalid tool id: {error}")))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The name models see (`fs.read` → `fs_read`): providers only accept
    /// `[A-Za-z0-9_-]` in function names. The registry rejects collisions.
    pub fn model_name(&self) -> String {
        self.as_str().replace('.', "_")
    }

    /// The id as an [`EntityId`] for events and policy intents.
    pub fn entity_id(&self) -> &EntityId {
        &self.0
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for ToolId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ToolId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// What a tool can do, reported to policy evaluation and approval cards.
///
/// These are the tool's *potential* effects; a [`crate::ToolPlan`]'s facts state
/// what one particular call will do (for example, a recursive delete is destructive
/// while deleting one file is not).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RiskProfile {
    /// The brokered action this tool performs.
    pub action: Action,
    /// Risk before looking at a particular call's facts.
    pub base_risk: RiskLevel,
    pub reads_workspace: bool,
    pub writes_workspace: bool,
    pub spawns_process: bool,
    pub network: bool,
    pub destructive: bool,
    /// Writes to a remote system (for example `git push`).
    pub remote_write: bool,
}

impl RiskProfile {
    /// A read-only workspace tool (no writes, network, or destructive effect).
    pub fn read_only(action: Action) -> Self {
        Self {
            action,
            base_risk: RiskLevel::Low,
            reads_workspace: true,
            writes_workspace: false,
            spawns_process: false,
            network: false,
            destructive: false,
            remote_write: false,
        }
    }

    /// Whether a broker may auto-permit the tool without a policy write decision:
    /// it neither writes, contacts the network, nor destroys anything. Read-only
    /// tools may still spawn fixed, read-only processes (`git status`).
    pub fn is_read_only(&self) -> bool {
        !(self.writes_workspace || self.network || self.destructive || self.remote_write)
    }
}

/// Everything the registry, broker, and model need to know about a tool.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolDescriptor {
    pub id: ToolId,
    /// Incremented whenever the input/output contract or behaviour changes.
    pub version: u32,
    pub description: String,
    /// JSON Schema (draft 7, closed objects) generated from the typed input struct.
    pub input_schema: Value,
    /// JSON Schema generated from the typed output struct.
    pub output_schema: Value,
    pub risk: RiskProfile,
    /// Capability tags (`workspace:read`, `workspace:write`, `process:git`, ...).
    pub capabilities: Vec<String>,
}

impl ToolDescriptor {
    /// A descriptor whose schemas are generated from `I` (input) and `O` (output).
    pub fn new<I: JsonSchema, O: JsonSchema>(
        id: ToolId,
        version: u32,
        description: impl Into<String>,
        risk: RiskProfile,
        capabilities: &[&str],
    ) -> Self {
        Self {
            id,
            version,
            description: description.into(),
            input_schema: schema::schema_of::<I>(),
            output_schema: schema::schema_of::<O>(),
            risk,
            capabilities: capabilities.iter().map(|tag| (*tag).to_owned()).collect(),
        }
    }

    /// The model-facing function definition.
    pub fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.id.model_name(),
            description: self.description.clone(),
            parameters: self.input_schema.clone(),
        }
    }
}

/// A model-facing function definition: provider-safe `name`, `description`, and the
/// JSON Schema of the `parameters`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}
