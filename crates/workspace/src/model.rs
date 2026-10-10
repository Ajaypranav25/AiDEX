//! Workspace records and their transport-safe JSON shapes.

use aidex_core::{EntityId, Timestamp};
use aidex_drive::RelativePath;
use serde::{Deserialize, Serialize};

use crate::WorkspaceError;

/// Prefix of generated workspace identifiers (`wsp_<ULID>`).
pub const WORKSPACE_ID_PREFIX: &str = "wsp";
/// Maximum workspace name length in characters.
pub const MAX_NAME_CHARS: usize = 64;

/// A registered workspace as seen on this host.
///
/// `availability`, `path`, and `repository` are computed when the record is read;
/// everything else is persisted in the registry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    /// Stable identity used by tool requests (`wsp_<ULID>`).
    pub id: EntityId,
    /// Unique (case-insensitive) human label.
    pub name: String,
    pub kind: WorkspaceKind,
    /// Persisted root: an absolute host path, or a path relative to the drive's
    /// `workspaces/` directory.
    pub root: WorkspaceRoot,
    pub created_at: Timestamp,
    pub last_opened_at: Option<Timestamp>,
    /// The root's absolute location on this host (without a Windows verbatim prefix).
    pub path: String,
    pub availability: WorkspaceAvailability,
    /// The repository containing the root, when the root is available and inside one.
    pub repository: Option<RepositoryInfo>,
}

impl Workspace {
    pub fn is_available(&self) -> bool {
        self.availability == WorkspaceAvailability::Available
    }
}

/// Where a workspace's files live and how AiDEX may treat them.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceKind {
    /// A directory the user selected on the host. AiDEX never deletes it.
    Host,
    /// A directory AiDEX created on the drive under `workspaces/<id>/tree`.
    Drive,
    /// A reviewable copy of `source` under `workspaces/ephemeral/<id>/tree`.
    Ephemeral { source: EntityId },
}

impl WorkspaceKind {
    /// Stable label: `host`, `drive`, or `ephemeral`.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Drive => "drive",
            Self::Ephemeral { .. } => "ephemeral",
        }
    }
}

/// A persisted workspace root.
///
/// Drive roots are relative to `DrivePaths::workspaces()` so they survive drive-letter
/// and mount-point changes; host roots are canonical absolute paths on the host
/// where they were added.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "path", rename_all = "snake_case")]
pub enum WorkspaceRoot {
    Host(String),
    Drive(RelativePath),
}

impl WorkspaceRoot {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Host(path) => path,
            Self::Drive(path) => path.as_str(),
        }
    }
}

/// Whether the root can be used on this host right now. A missing host root is
/// reported, never recreated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceAvailability {
    Available,
    Unavailable { reason: String },
}

/// Version-control system detected for a directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vcs {
    Git,
    None,
}

/// The repository that contains a workspace root.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryInfo {
    pub vcs: Vcs,
    /// Absolute repository root on this host (display form).
    pub root: String,
    /// Whether the workspace root is the repository root (rather than inside it).
    pub is_workspace_root: bool,
}

/// Validate and normalize a workspace name: trimmed, 1-64 characters, no control
/// characters, and not shaped like a generated workspace id.
pub fn validate_name(name: &str) -> Result<String, WorkspaceError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(WorkspaceError::InvalidName(
            "the name must not be empty".into(),
        ));
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(WorkspaceError::InvalidName(format!(
            "the name must be at most {MAX_NAME_CHARS} characters"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(WorkspaceError::InvalidName(
            "the name must not contain control characters".into(),
        ));
    }
    if trimmed
        .get(..WORKSPACE_ID_PREFIX.len() + 1)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("wsp_"))
    {
        return Err(WorkspaceError::InvalidName(
            "names starting with `wsp_` are reserved for workspace ids".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

/// Case-insensitive uniqueness key for a validated name.
pub(crate) fn name_key(name: &str) -> String {
    name.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_are_trimmed_bounded_and_not_id_shaped() {
        assert_eq!(validate_name("  my repo ").unwrap(), "my repo");
        for bad in ["", "   ", "a\nb", "wsp_01ARZ", "WSP_x"] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
        assert!(validate_name(&"x".repeat(MAX_NAME_CHARS)).is_ok());
        assert!(validate_name(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
        assert!(validate_name("wsp").is_ok());
    }

    #[test]
    fn kinds_roots_and_availability_have_stable_json_shapes() {
        let source = EntityId::new("wsp_source").unwrap();
        assert_eq!(
            serde_json::to_value(WorkspaceKind::Host).unwrap(),
            json!({"type": "host"})
        );
        assert_eq!(
            serde_json::to_value(WorkspaceKind::Ephemeral { source }).unwrap(),
            json!({"type": "ephemeral", "source": "wsp_source"})
        );
        assert_eq!(
            serde_json::to_value(WorkspaceRoot::Drive(
                RelativePath::new("wsp_a/tree").unwrap()
            ))
            .unwrap(),
            json!({"type": "drive", "path": "wsp_a/tree"})
        );
        assert_eq!(
            serde_json::to_value(WorkspaceAvailability::Unavailable {
                reason: "gone".into()
            })
            .unwrap(),
            json!({"state": "unavailable", "reason": "gone"})
        );
        assert!(
            serde_json::from_value::<WorkspaceRoot>(json!({"type": "drive", "path": "../x"}))
                .is_err()
        );
    }
}
