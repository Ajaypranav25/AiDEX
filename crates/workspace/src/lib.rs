//! Workspace registry and confinement for AiDEX: explicitly selected host
//! directories, drive-resident workspaces, and reviewable ephemeral copies.
//!
//! - [`WorkspaceRegistry`] persists workspaces in the drive's
//!   `private_config/workspaces.sqlite3` and hands out [`WorkspaceHandle`]s.
//! - [`ConfinedRoot`] resolves workspace-relative paths so they cannot escape the
//!   root through traversal, symbolic links, or junctions.
//! - [`discover`] reports the enclosing and nested repositories of a directory.
//! - Ephemeral workspaces copy a source (honoring `.gitignore`), record a baseline,
//!   and support review, selective apply with conflict detection, and discard.

mod confine;
mod discover;
mod ephemeral;
mod error;
mod fsutil;
mod marker;
mod model;
mod platform;
mod registry;
mod walk;

pub use confine::{Access, ConfinedRoot, parse_workspace_path};
pub use discover::{Discovery, MAX_SCAN_DEPTH, MAX_SCAN_ENTRIES, discover};
pub use ephemeral::{
    ApplyFailure, ApplyReport, ApplySelection, BASELINE_FILE, ChangeKind, ConflictReason,
    EphemeralCreated, EphemeralDiff, EphemeralLimits, FileChange,
};
pub use error::WorkspaceError;
pub use marker::MARKER_FILE;
pub use model::{
    MAX_NAME_CHARS, RepositoryInfo, Vcs, WORKSPACE_ID_PREFIX, Workspace, WorkspaceAvailability,
    WorkspaceKind, WorkspaceRoot, validate_name,
};
pub use platform::{display_path, windows_component_issue};
pub use registry::{EPHEMERAL_DIR, REGISTRY_FILE, TREE_DIR, WorkspaceHandle, WorkspaceRegistry};
pub use walk::{SkipReason, SkippedEntry};
