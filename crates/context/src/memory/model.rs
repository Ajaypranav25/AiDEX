//! Memory entries, scopes, validation, and the pluggable store interface.

use std::fmt;

use aidex_core::{EntityId, ServiceErrorCode, Timestamp};
use serde::{Deserialize, Serialize};

use crate::exclude;

/// Prefix of generated memory ids (`mem_<ULID>`).
pub const MEMORY_ID_PREFIX: &str = "mem";
/// Longest memory text, in bytes (after trimming).
pub const MAX_MEMORY_TEXT_BYTES: usize = 4 * 1024;
/// Most tags per entry.
pub const MAX_TAGS: usize = 16;
/// Longest tag, in characters.
pub const MAX_TAG_CHARS: usize = 32;
/// Most entries in one scope.
pub const MAX_ENTRIES_PER_SCOPE: usize = 1_000;

/// Where a memory lives (spec §35).
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum MemoryScope {
    /// Portable user preferences carried on the drive across projects.
    User,
    /// Facts about one workspace (build commands, conventions, decisions).
    Workspace { workspace_id: EntityId },
    /// Notes for one session only.
    Session { session_id: EntityId },
}

impl MemoryScope {
    pub fn kind(&self) -> MemoryScopeKind {
        match self {
            Self::User => MemoryScopeKind::User,
            Self::Workspace { .. } => MemoryScopeKind::Workspace,
            Self::Session { .. } => MemoryScopeKind::Session,
        }
    }
}

/// The scope without its subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScopeKind {
    User,
    Workspace,
    Session,
}

impl fmt::Display for MemoryScopeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::User => "user",
            Self::Workspace => "workspace",
            Self::Session => "session",
        })
    }
}

/// Who last wrote a memory's text. Agent-written memories are agent-generated
/// text and are treated as untrusted data in context packs.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MemorySource {
    User,
    Agent { session_id: EntityId },
}

impl MemorySource {
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent { .. })
    }
}

/// One inspectable, editable memory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryEntry {
    pub id: EntityId,
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub source: MemorySource,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A memory to add.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewMemory {
    pub text: String,
    pub tags: Vec<String>,
    pub source: MemorySource,
}

impl NewMemory {
    /// A memory the user wrote.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tags: Vec::new(),
            source: MemorySource::User,
        }
    }

    /// A memory an agent wrote during `session_id`.
    pub fn agent(session_id: EntityId, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tags: Vec::new(),
            source: MemorySource::Agent { session_id },
        }
    }

    #[must_use]
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }
}

/// Changes to a memory. `by` becomes the entry's source, so an agent edit of a
/// user memory marks it agent-written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryEdit {
    pub text: Option<String>,
    pub tags: Option<Vec<String>>,
    pub by: MemorySource,
}

impl MemoryEdit {
    pub fn by_user() -> Self {
        Self {
            text: None,
            tags: None,
            by: MemorySource::User,
        }
    }

    #[must_use]
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    #[must_use]
    pub fn tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags = Some(tags.into_iter().map(Into::into).collect());
        self
    }
}

/// A store of memories. Implementations must reject secret-like text
/// ([`validate_text`]) and keep entries inspectable and editable.
pub trait MemoryStore: Send + Sync {
    fn list(&self, scope: &MemoryScope) -> Result<Vec<MemoryEntry>, MemoryError>;
    fn add(&self, scope: &MemoryScope, memory: NewMemory) -> Result<MemoryEntry, MemoryError>;
    fn edit(
        &self,
        scope: &MemoryScope,
        id: &EntityId,
        edit: MemoryEdit,
    ) -> Result<MemoryEntry, MemoryError>;
    fn remove(&self, scope: &MemoryScope, id: &EntityId) -> Result<MemoryEntry, MemoryError>;
    /// Remove every entry in the scope (e.g. when a session ends); returns how many.
    fn clear(&self, scope: &MemoryScope) -> Result<usize, MemoryError>;
}

/// Memory failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryError {
    /// The text (or a tag) looks like a credential. Memory is not a secret store;
    /// put secrets in the vault.
    SecretLike,
    InvalidText(String),
    InvalidTag(String),
    InvalidScope(String),
    NotFound(EntityId),
    TooMany {
        max: usize,
    },
    /// A memory file exists but does not parse (it may have been edited by hand).
    Corrupt {
        file: String,
        message: String,
    },
    Io(String),
}

impl MemoryError {
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::SecretLike
            | Self::InvalidText(_)
            | Self::InvalidTag(_)
            | Self::InvalidScope(_)
            | Self::TooMany { .. } => ServiceErrorCode::InvalidArgument,
            Self::NotFound(_) => ServiceErrorCode::NotFound,
            Self::Corrupt { .. } | Self::Io(_) => ServiceErrorCode::Internal,
        }
    }

    pub fn reason(&self) -> &'static str {
        match self {
            Self::SecretLike => "secret_like",
            Self::InvalidText(_) => "invalid_text",
            Self::InvalidTag(_) => "invalid_tag",
            Self::InvalidScope(_) => "invalid_scope",
            Self::NotFound(_) => "memory_not_found",
            Self::TooMany { .. } => "too_many_memories",
            Self::Corrupt { .. } => "corrupt_memory",
            Self::Io(_) => "io",
        }
    }
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SecretLike => f.write_str(
                "the memory looks like it contains a secret; memory is not a secret store (use the vault)",
            ),
            Self::InvalidText(message) => write!(f, "invalid memory text: {message}"),
            Self::InvalidTag(message) => write!(f, "invalid memory tag: {message}"),
            Self::InvalidScope(message) => write!(f, "invalid memory scope: {message}"),
            Self::NotFound(id) => write!(f, "memory `{id}` was not found"),
            Self::TooMany { max } => write!(f, "a memory scope holds at most {max} entries"),
            Self::Corrupt { file, message } => {
                write!(f, "memory file `{file}` is invalid: {message}")
            }
            Self::Io(message) => write!(f, "memory storage failed: {message}"),
        }
    }
}

impl std::error::Error for MemoryError {}

/// Trim and validate memory text: 1..=[`MAX_MEMORY_TEXT_BYTES`] bytes, no control
/// characters other than newlines and tabs, nothing credential-like.
pub fn validate_text(text: &str) -> Result<String, MemoryError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(MemoryError::InvalidText("the text is empty".into()));
    }
    if text.len() > MAX_MEMORY_TEXT_BYTES {
        return Err(MemoryError::InvalidText(format!(
            "the text is longer than {MAX_MEMORY_TEXT_BYTES} bytes"
        )));
    }
    if text
        .chars()
        .any(|ch| ch.is_control() && ch != '\n' && ch != '\t' && ch != '\r')
    {
        return Err(MemoryError::InvalidText(
            "the text contains control characters".into(),
        ));
    }
    if exclude::contains_secret_like(text) {
        return Err(MemoryError::SecretLike);
    }
    Ok(text.to_owned())
}

/// Normalize tags: trimmed, lowercase `[a-z0-9._-]`, at most [`MAX_TAGS`],
/// deduplicated in order.
pub fn validate_tags(tags: &[String]) -> Result<Vec<String>, MemoryError> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() || tag.chars().count() > MAX_TAG_CHARS {
            return Err(MemoryError::InvalidTag(format!(
                "tags must be 1-{MAX_TAG_CHARS} characters"
            )));
        }
        if !tag
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        {
            return Err(MemoryError::InvalidTag(format!(
                "`{tag}` may only contain a-z, 0-9, `.`, `_`, and `-`"
            )));
        }
        if exclude::contains_secret_like(&tag) {
            return Err(MemoryError::SecretLike);
        }
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    if out.len() > MAX_TAGS {
        return Err(MemoryError::InvalidTag(format!(
            "at most {MAX_TAGS} tags are allowed"
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_trimmed_bounded_and_secret_free() {
        assert_eq!(validate_text("  use bun  ").unwrap(), "use bun");
        assert!(matches!(
            validate_text("   "),
            Err(MemoryError::InvalidText(_))
        ));
        assert!(validate_text(&"x".repeat(MAX_MEMORY_TEXT_BYTES + 1)).is_err());
        assert!(validate_text("bell\u{7}").is_err());
        let token = format!("{}{}", "ghp_", "FAKE0fake1FAKE2fake3FAKE4fake5");
        assert_eq!(
            validate_text(&format!("the deploy token is {token}")),
            Err(MemoryError::SecretLike)
        );
        assert_eq!(
            validate_text(&format!("{}=hunter22", "PASSWORD")),
            Err(MemoryError::SecretLike)
        );
    }

    #[test]
    fn tags_are_normalized() {
        let tags = vec![" Build ".to_owned(), "build".to_owned(), "ci-1".to_owned()];
        assert_eq!(validate_tags(&tags).unwrap(), ["build", "ci-1"]);
        assert!(validate_tags(&["has space".to_owned()]).is_err());
        assert!(validate_tags(&[String::new()]).is_err());
    }

    #[test]
    fn scopes_and_sources_have_stable_json() {
        let scope = MemoryScope::Workspace {
            workspace_id: EntityId::new("wsp_a").unwrap(),
        };
        assert_eq!(
            serde_json::to_value(&scope).unwrap(),
            serde_json::json!({"scope": "workspace", "workspace_id": "wsp_a"})
        );
        let source = MemorySource::Agent {
            session_id: EntityId::new("ses_a").unwrap(),
        };
        assert_eq!(
            serde_json::to_value(&source).unwrap(),
            serde_json::json!({"kind": "agent", "session_id": "ses_a"})
        );
        assert_eq!(MemoryError::SecretLike.reason(), "secret_like");
    }
}
