//! Normalized intents: what is being asked for, by whom, with broker-computed facts.

use std::fmt;

use aidex_core::EntityId;
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    Action, NetworkTarget, ShellClassification, ShellPlatform, classify_shell, macros::string_enum,
    redact_command,
};

/// A request to perform one brokered action.
///
/// Every attribute is a fact the broker computed (resolved paths, classified
/// commands, classified destinations), never a claim copied from agent output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub action: Action,
    pub session_id: EntityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<EntityId>,
    /// Broker-written human summary of the request (shown on approval cards).
    pub summary: String,
    #[serde(default)]
    pub attributes: IntentAttributes,
}

/// Broker-computed facts about an intent.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentAttributes {
    /// Filesystem targets, resolved and confined by the workspace service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<PathScope>,
    /// The classified command line for `shell.exec` / `process.spawn`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<ShellCommand>,
    /// The classified destination for network-bearing actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<SecretUse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_locality: Option<ModelLocality>,
    /// The action writes to a remote system (push, publish, remote MCP mutation).
    #[serde(default)]
    pub remote_write: bool,
    /// The action has an irreversible destructive effect.
    #[serde(default)]
    pub destructive: bool,
}

string_enum! {
    /// Sensitivity of a filesystem target.
    pub enum PathClass("path class") {
        Ordinary => "ordinary",
        /// Credential-bearing files (`.env`, SSH keys, cloud credentials).
        Sensitive => "sensitive",
        /// AiDEX-managed private state (vault, identity, broker runtime, policy store).
        Protected => "protected",
    }
}

string_enum! {
    /// Where a model runs.
    pub enum ModelLocality("model locality") {
        Local => "local",
        Remote => "remote",
    }
}

/// One filesystem target of an intent.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathScope {
    /// The resolved path, as displayed on approval cards.
    pub path: String,
    /// Workspace-relative path with `/` separators when the target is inside the
    /// intent's workspace; `None` when it is outside. `""` is the workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_relative: Option<String>,
    #[serde(default = "ordinary")]
    pub class: PathClass,
}

fn ordinary() -> PathClass {
    PathClass::Ordinary
}

impl PathScope {
    /// A target inside the workspace at `relative` (`/`-separated).
    pub fn inside(path: impl Into<String>, relative: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            workspace_relative: Some(relative.into()),
            class: PathClass::Ordinary,
        }
    }

    /// A target outside every workspace boundary.
    pub fn outside(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            workspace_relative: None,
            class: PathClass::Ordinary,
        }
    }

    pub fn with_class(mut self, class: PathClass) -> Self {
        self.class = class;
        self
    }

    pub fn inside_workspace(&self) -> bool {
        self.workspace_relative.is_some()
    }
}

/// A secret the action needs, by vault name, and who will receive it.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretUse {
    /// Vault entry name (never the value).
    pub name: String,
    /// The tool, MCP server, or provider that receives the secret.
    pub consumer: String,
}

impl SecretUse {
    pub fn new(name: impl Into<String>, consumer: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            consumer: consumer.into(),
        }
    }
}

/// A command line with the classification computed from it.
///
/// The classification is always derived by [`classify_shell`]: construction and
/// deserialization re-classify the line, so a caller cannot supply a lower risk.
/// `Debug` and [`ShellCommand::redacted_line`] hide likely credentials; the raw line
/// is still serialized and must not be logged.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ShellCommand {
    line: String,
    platform: ShellPlatform,
    classification: ShellClassification,
}

impl ShellCommand {
    pub fn classify(line: impl Into<String>, platform: ShellPlatform) -> Self {
        let line = line.into();
        let classification = classify_shell(&line, platform);
        Self {
            line,
            platform,
            classification,
        }
    }

    /// The raw command line. Treat as sensitive.
    pub fn line(&self) -> &str {
        &self.line
    }

    /// The command line with likely credentials removed, for display and audit.
    pub fn redacted_line(&self) -> String {
        redact_command(&self.line)
    }

    pub fn platform(&self) -> ShellPlatform {
        self.platform
    }

    pub fn classification(&self) -> &ShellClassification {
        &self.classification
    }
}

impl fmt::Debug for ShellCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShellCommand")
            .field("line", &self.redacted_line())
            .field("platform", &self.platform)
            .field("classification", &self.classification)
            .finish()
    }
}

impl<'de> Deserialize<'de> for ShellCommand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            line: String,
            platform: ShellPlatform,
            #[serde(default, rename = "classification")]
            _classification: Option<de::IgnoredAny>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Ok(Self::classify(wire.line, wire.platform))
    }
}

impl Intent {
    pub fn new(action: Action, session_id: EntityId, summary: impl Into<String>) -> Self {
        Self {
            action,
            session_id,
            workspace_id: None,
            agent_id: None,
            tool_id: None,
            skill_id: None,
            summary: summary.into(),
            attributes: IntentAttributes::default(),
        }
    }

    pub fn with_workspace(mut self, id: EntityId) -> Self {
        self.workspace_id = Some(id);
        self
    }

    pub fn with_agent(mut self, id: EntityId) -> Self {
        self.agent_id = Some(id);
        self
    }

    pub fn with_tool(mut self, id: EntityId) -> Self {
        self.tool_id = Some(id);
        self
    }

    pub fn with_skill(mut self, id: EntityId) -> Self {
        self.skill_id = Some(id);
        self
    }

    pub fn with_path(mut self, path: PathScope) -> Self {
        self.attributes.paths.push(path);
        self
    }

    pub fn with_command(mut self, command: ShellCommand) -> Self {
        self.attributes.command = Some(command);
        self
    }

    pub fn with_network(mut self, target: NetworkTarget) -> Self {
        self.attributes.network = Some(target);
        self
    }

    pub fn with_secret(mut self, secret: SecretUse) -> Self {
        self.attributes.secret = Some(secret);
        self
    }

    pub fn with_model_locality(mut self, locality: ModelLocality) -> Self {
        self.attributes.model_locality = Some(locality);
        self
    }

    pub fn with_remote_write(mut self) -> Self {
        self.attributes.remote_write = true;
        self
    }

    pub fn with_destructive(mut self) -> Self {
        self.attributes.destructive = true;
        self
    }
}
