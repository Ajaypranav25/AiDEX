//! The closed catalogue of brokered actions and patterns that select them.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{PolicyError, macros::string_enum};

string_enum! {
    /// Every operation an agent, tool, skill, or remote client can ask the broker for.
    ///
    /// The set is closed: an action id that does not parse is an error, never an
    /// "unknown but allowed" action.
    pub enum Action("action") {
        FsRead => "fs.read",
        FsWrite => "fs.write",
        FsDelete => "fs.delete",
        FsSearch => "fs.search",
        ShellExec => "shell.exec",
        ProcessSpawn => "process.spawn",
        NetworkRequest => "network.request",
        PackageInstall => "package.install",
        /// Read-only git inspection (status, diff, log, show).
        GitRead => "git.read",
        GitCommit => "git.commit",
        GitBranch => "git.branch",
        GitPush => "git.push",
        GitPull => "git.pull",
        SecretUse => "secret.use",
        McpCall => "mcp.call",
        AgentSpawn => "agent.spawn",
        /// Model invocation; locality is carried by the intent attributes.
        ModelInvoke => "model.invoke",
        ArtifactCreate => "artifact.create",
        HostMount => "host.mount",
        RemoteAction => "remote.action",
        PolicyChange => "policy.change",
        VaultModify => "vault.modify",
        BrokerDisable => "broker.disable",
        DiskFormat => "disk.format",
        DriveRawWrite => "drive.raw_write",
        CredentialExport => "credential.export",
        SecretDump => "secret.dump",
        DirectoryDeleteArbitrary => "directory.delete_arbitrary",
    }
}

impl Action {
    /// The id prefix before the first `.` (`fs`, `git`, ...).
    pub fn category(self) -> &'static str {
        let id = self.as_str();
        id.split_once('.').map_or(id, |(category, _)| category)
    }

    /// Whether a hard safety rule denies this action unconditionally.
    pub const fn is_hard_denied(self) -> bool {
        matches!(
            self,
            Self::VaultModify
                | Self::BrokerDisable
                | Self::DiskFormat
                | Self::DriveRawWrite
                | Self::CredentialExport
                | Self::SecretDump
                | Self::DirectoryDeleteArbitrary
        )
    }

    /// Whether the action targets filesystem paths and therefore needs path facts.
    pub const fn is_filesystem(self) -> bool {
        matches!(
            self,
            Self::FsRead | Self::FsWrite | Self::FsDelete | Self::FsSearch
        )
    }

    /// Whether the action modifies filesystem content.
    pub const fn is_filesystem_write(self) -> bool {
        matches!(self, Self::FsWrite | Self::FsDelete)
    }

    /// Whether the action runs a command line and therefore needs a classified command.
    pub const fn is_command(self) -> bool {
        matches!(self, Self::ShellExec | Self::ProcessSpawn)
    }

    /// Short human-readable description used in effect summaries.
    pub const fn label(self) -> &'static str {
        match self {
            Self::FsRead => "Read files",
            Self::FsWrite => "Write files",
            Self::FsDelete => "Delete files",
            Self::FsSearch => "Search files",
            Self::ShellExec => "Run a shell command",
            Self::ProcessSpawn => "Start a process",
            Self::NetworkRequest => "Make a network request",
            Self::PackageInstall => "Install packages",
            Self::GitRead => "Read git history and status",
            Self::GitCommit => "Create git commits",
            Self::GitBranch => "Create or switch git branches",
            Self::GitPush => "Upload commits to a remote repository",
            Self::GitPull => "Download changes from a remote repository",
            Self::SecretUse => "Use a stored secret",
            Self::McpCall => "Call an MCP tool",
            Self::AgentSpawn => "Start an agent",
            Self::ModelInvoke => "Invoke a model",
            Self::ArtifactCreate => "Create an artifact",
            Self::HostMount => "Expose a host directory to the session",
            Self::RemoteAction => "Perform a remote-control action",
            Self::PolicyChange => "Change security policy",
            Self::VaultModify => "Modify the AiDEX vault",
            Self::BrokerDisable => "Disable the execution broker",
            Self::DiskFormat => "Format or repartition a disk",
            Self::DriveRawWrite => "Write raw data to a drive",
            Self::CredentialExport => "Export credentials",
            Self::SecretDump => "Dump stored secrets",
            Self::DirectoryDeleteArbitrary => "Delete a root, home, or system directory",
        }
    }
}

/// Selects actions in a rule: one action (`fs.write`), a category (`git.*`), or all (`*`).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ActionPattern {
    Exact(Action),
    Category(&'static str),
    Any,
}

impl ActionPattern {
    pub fn matches(&self, action: Action) -> bool {
        match self {
            Self::Exact(expected) => *expected == action,
            Self::Category(category) => action.category() == *category,
            Self::Any => true,
        }
    }
}

impl From<Action> for ActionPattern {
    fn from(action: Action) -> Self {
        Self::Exact(action)
    }
}

impl FromStr for ActionPattern {
    type Err = PolicyError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "*" {
            return Ok(Self::Any);
        }
        if let Some(category) = value.strip_suffix(".*") {
            return Action::ALL
                .iter()
                .map(|action| action.category())
                .find(|known| *known == category)
                .map(Self::Category)
                .ok_or_else(|| PolicyError::unknown("action category", value));
        }
        value.parse().map(Self::Exact)
    }
}

impl fmt::Display for ActionPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(action) => f.write_str(action.as_str()),
            Self::Category(category) => write!(f, "{category}.*"),
            Self::Any => f.write_str("*"),
        }
    }
}

impl Serialize for ActionPattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ActionPattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}
