//! Validation of user-selected host roots against the AiDEX drive.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use aidex_drive::DrivePaths;

use crate::{WorkspaceError, platform};

/// Canonicalize an explicitly selected host directory and refuse locations that
/// would expose AiDEX state: the drive root, any directory containing the drive
/// (its vault, identity, and sessions would become workspace files), and anything
/// inside the drive's managed directories or `.aidex`.
pub(crate) fn validate_host_root(
    paths: &DrivePaths,
    path: &Path,
) -> Result<PathBuf, WorkspaceError> {
    let shown = || path.display().to_string();
    if !path.is_absolute() {
        return Err(WorkspaceError::invalid_path(
            shown(),
            "an absolute path is required (relative paths depend on the daemon's working directory)",
        ));
    }
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            WorkspaceError::invalid_path(shown(), "the directory does not exist")
        } else {
            WorkspaceError::io(format!("cannot inspect `{}`", shown()), error)
        }
    })?;
    if !metadata.is_dir() {
        return Err(WorkspaceError::invalid_path(shown(), "not a directory"));
    }
    let canonical = fs::canonicalize(path)
        .map_err(|error| WorkspaceError::io(format!("cannot resolve `{}`", shown()), error))?;
    check_not_drive_state(paths, &canonical)?;
    Ok(canonical)
}

/// Refuse a canonical host root that is, contains, or lies inside AiDEX-managed
/// drive state. Re-checked whenever a host root is resolved for use, because the
/// drive may have been moved since the workspace was added.
pub(crate) fn check_not_drive_state(
    paths: &DrivePaths,
    canonical: &Path,
) -> Result<(), WorkspaceError> {
    let shown = platform::display_path(canonical).display().to_string();
    let drive_root = fs::canonicalize(paths.root()).map_err(|error| {
        WorkspaceError::io(
            format!(
                "cannot resolve the AiDEX drive root `{}`",
                paths.root().display()
            ),
            error,
        )
    })?;
    if platform::is_within(&drive_root, canonical) {
        return Err(WorkspaceError::Forbidden(format!(
            "`{shown}` is or contains the AiDEX drive; select a project directory instead"
        )));
    }
    if !platform::is_within(canonical, &drive_root) {
        return Ok(());
    }
    let managed = paths
        .layout()
        .entries()
        .map(|(_, relative)| relative.resolve_under(&drive_root))
        .chain([drive_root.join(".aidex")]);
    for directory in managed {
        let directory = fs::canonicalize(&directory).unwrap_or(directory);
        if platform::is_within(canonical, &directory) {
            return Err(WorkspaceError::Forbidden(format!(
                "`{shown}` is inside the AiDEX-managed directory `{}`; use `create_drive` for drive workspaces",
                platform::display_path(&directory).display()
            )));
        }
    }
    Ok(())
}
