//! Typed consumer scopes: who may receive a secret through a handle (spec §15.3, §92).
//!
//! A scope is `<kind>:<target>` with kind `provider`, `mcp`, `process`, or `git`.
//! Model prompts, agents, sessions, logs, and shell environments are never valid
//! consumers; their prefixes are rejected explicitly rather than treated as unknown.

use crate::VaultError;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{fmt, str::FromStr};

/// Maximum length of a scope target in bytes.
pub const MAX_TARGET_LEN: usize = 128;

/// Prefixes that must never receive secrets.
const FORBIDDEN_KINDS: &[&str] = &[
    "model",
    "models",
    "prompt",
    "llm",
    "agent",
    "agents",
    "session",
    "sessions",
    "context",
    "memory",
    "log",
    "logs",
    "event",
    "events",
    "env",
    "environment",
    "shell",
];

/// The kind of component a secret may be handed to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ConsumerKind {
    /// A model provider adapter, e.g. `provider:openai`.
    Provider,
    /// An MCP server launched or connected by the broker, e.g. `mcp:github`.
    Mcp,
    /// A brokered process for a specific tool, e.g. `process:git.push`.
    Process,
    /// Git credential use for one remote host, e.g. `git:github.com`.
    Git,
}

impl ConsumerKind {
    pub const ALL: [Self; 4] = [Self::Provider, Self::Mcp, Self::Process, Self::Git];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Mcp => "mcp",
            Self::Process => "process",
            Self::Git => "git",
        }
    }
}

/// A validated consumer scope. Matching is exact; there are no wildcards.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ConsumerScope {
    kind: ConsumerKind,
    target: String,
}

impl ConsumerScope {
    pub fn new(kind: ConsumerKind, target: impl Into<String>) -> Result<Self, VaultError> {
        let target = target.into();
        let valid = match kind {
            ConsumerKind::Git => is_remote_host(&target),
            ConsumerKind::Provider | ConsumerKind::Mcp | ConsumerKind::Process => {
                is_identifier(&target)
            }
        };
        if !valid {
            let expected = match kind {
                ConsumerKind::Git => "a lowercase host name with an optional `:port`",
                _ => {
                    "1-128 characters of lowercase `a-z`, `0-9`, `.`, `_`, `-`, starting with a letter or digit"
                }
            };
            return Err(VaultError::InvalidConsumer(format!(
                "`{}:{target}` target must be {expected}",
                kind.as_str()
            )));
        }
        Ok(Self { kind, target })
    }

    pub fn provider(id: impl Into<String>) -> Result<Self, VaultError> {
        Self::new(ConsumerKind::Provider, id)
    }

    pub fn mcp(server_id: impl Into<String>) -> Result<Self, VaultError> {
        Self::new(ConsumerKind::Mcp, server_id)
    }

    pub fn process(tool_id: impl Into<String>) -> Result<Self, VaultError> {
        Self::new(ConsumerKind::Process, tool_id)
    }

    pub fn git(remote_host: impl Into<String>) -> Result<Self, VaultError> {
        Self::new(ConsumerKind::Git, remote_host)
    }

    /// Parse `<kind>:<target>`.
    pub fn parse(text: &str) -> Result<Self, VaultError> {
        let Some((kind, target)) = text.split_once(':') else {
            return Err(VaultError::InvalidConsumer(format!(
                "`{text}` must look like `<kind>:<target>` with kind provider, mcp, process, or git"
            )));
        };
        if FORBIDDEN_KINDS.contains(&kind.to_ascii_lowercase().as_str()) {
            return Err(VaultError::ForbiddenConsumer(text.to_owned()));
        }
        let kind = ConsumerKind::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == kind)
            .ok_or_else(|| {
                VaultError::InvalidConsumer(format!(
                    "unknown consumer kind `{kind}` (expected provider, mcp, process, or git)"
                ))
            })?;
        Self::new(kind, target)
    }

    pub fn kind(&self) -> ConsumerKind {
        self.kind
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}

fn is_identifier(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=MAX_TARGET_LEN).contains(&bytes.len())
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|&b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// `host` or `host:port`: dot-separated lowercase DNS labels.
fn is_remote_host(text: &str) -> bool {
    if text.is_empty() || text.len() > MAX_TARGET_LEN {
        return false;
    }
    let (host, port) = match text.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (text, None),
    };
    let port_ok = port.is_none_or(|port| {
        !port.is_empty()
            && port.len() <= 5
            && port.bytes().all(|b| b.is_ascii_digit())
            && port.parse::<u16>().is_ok_and(|value| value != 0)
    });
    let labels_ok = host.split('.').all(|label| {
        let bytes = label.as_bytes();
        (1..=63).contains(&bytes.len())
            && bytes
                .iter()
                .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && bytes[0] != b'-'
            && bytes[bytes.len() - 1] != b'-'
    });
    port_ok && labels_ok
}

impl fmt::Display for ConsumerScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind.as_str(), self.target)
    }
}

impl FromStr for ConsumerScope {
    type Err = VaultError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl Serialize for ConsumerScope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ConsumerScope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VaultErrorKind;

    #[test]
    fn valid_scopes_round_trip_through_text_and_json() {
        for text in [
            "provider:openai",
            "mcp:github",
            "process:git.push",
            "git:github.com",
            "git:git.example.org:2222",
            "provider:local-llama_2",
        ] {
            let scope = ConsumerScope::parse(text).unwrap();
            assert_eq!(scope.to_string(), text);
            let json = serde_json::to_string(&scope).unwrap();
            assert_eq!(serde_json::from_str::<ConsumerScope>(&json).unwrap(), scope);
        }
        let scope = ConsumerScope::provider("openai").unwrap();
        assert_eq!(
            (scope.kind(), scope.target()),
            (ConsumerKind::Provider, "openai")
        );
    }

    #[test]
    fn model_prompts_and_shell_environments_are_never_consumers() {
        for text in [
            "model:gpt",
            "prompt:system",
            "agent:native",
            "session:abc",
            "env:OPENAI_API_KEY",
            "shell:bash",
            "log:daemon",
            "Model:x",
        ] {
            let error = ConsumerScope::parse(text).unwrap_err();
            assert_eq!(error.kind(), VaultErrorKind::ForbiddenConsumer, "{text}");
        }
    }

    #[test]
    fn malformed_scopes_are_rejected() {
        for text in [
            "openai",
            "provider:",
            "provider:OpenAI",
            "provider:-x",
            "provider:a b",
            "unknown:x",
            "git:Github.com",
            "git:-bad.com",
            "git:host:0",
            "git:host:99999",
            "git:host:",
            "git:a..b",
            "mcp:github/x",
        ] {
            let error = ConsumerScope::parse(text).unwrap_err();
            assert_eq!(error.kind(), VaultErrorKind::InvalidArgument, "{text}");
        }
        let long = format!("provider:{}", "a".repeat(MAX_TARGET_LEN + 1));
        assert!(ConsumerScope::parse(&long).is_err());
        assert!(serde_json::from_str::<ConsumerScope>("\"env:HOME\"").is_err());
    }
}
