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

