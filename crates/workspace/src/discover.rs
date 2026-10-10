//! Repository discovery around an explicitly selected directory.
//!
//! Discovery only informs the user: the registered root is always exactly the
//! directory they selected, never a repository root AiDEX guessed.

use std::{
    collections::VecDeque,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use aidex_core::WorkspacePath;
use serde::{Deserialize, Serialize};

use crate::{
    RepositoryInfo, Vcs, WorkspaceError,
    platform::{self, display_path},
};

/// Deepest directory level (below the selected root) inspected for nested repositories.
pub const MAX_SCAN_DEPTH: usize = 4;
/// Maximum directory entries inspected while looking for nested repositories.
pub const MAX_SCAN_ENTRIES: usize = 2000;

/// What surrounds and lies beneath a selected directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discovery {
    /// The canonical directory the user selected (display form).
    pub selected_root: PathBuf,
    /// The nearest repository root at or above `selected_root`.
    pub enclosing_repository: Option<PathBuf>,
    /// Repository roots strictly below `selected_root`, relative to it, in sorted order.
    pub nested_repositories: Vec<WorkspacePath>,
    /// The version-control system of `enclosing_repository`.
    pub vcs: Vcs,
    /// The bounded scan stopped early ([`MAX_SCAN_ENTRIES`]); more nested
    /// repositories may exist.
    pub scan_truncated: bool,
}

/// Inspect an existing directory: its enclosing repository and a bounded scan
/// (depth [`MAX_SCAN_DEPTH`], [`MAX_SCAN_ENTRIES`] entries) for nested repositories.
/// Symbolic links and junctions are never followed during the scan.
pub fn discover(path: &Path) -> Result<Discovery, WorkspaceError> {
    let shown = || path.display().to_string();
    if !path.is_absolute() {
        return Err(WorkspaceError::invalid_path(
            shown(),
            "an absolute path is required",
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            WorkspaceError::invalid_path(shown(), "the directory does not exist")
        } else {
            WorkspaceError::io(format!("cannot resolve `{}`", shown()), error)
        }
    })?;
    if !canonical.is_dir() {
        return Err(WorkspaceError::invalid_path(shown(), "not a directory"));
    }
    let enclosing = enclosing_repository(&canonical, true);
    let (nested_repositories, scan_truncated) = nested_repositories(&canonical);
    Ok(Discovery {
        selected_root: display_path(&canonical),
        vcs: if enclosing.is_some() {
            Vcs::Git
        } else {
            Vcs::None
        },
        enclosing_repository: enclosing.map(|root| display_path(&root)),
        nested_repositories,
        scan_truncated,
    })
}

/// Whether `dir` is a Git repository root (`.git` directory or `gitdir:` file).
fn is_repository_root(dir: &Path) -> bool {
    fs::symlink_metadata(dir.join(".git"))
        .is_ok_and(|metadata| metadata.is_dir() || metadata.is_file())
}

/// The nearest repository root at `root` or, when `ancestors` is set, above it.
pub(crate) fn enclosing_repository(root: &Path, ancestors: bool) -> Option<PathBuf> {
    if !ancestors {
        return is_repository_root(root).then(|| root.to_path_buf());
    }
    root.ancestors()
        .find(|dir| is_repository_root(dir))
        .map(Path::to_path_buf)
}

/// Repository information for an available, canonical workspace root.
pub(crate) fn repository_info(root: &Path, ancestors: bool) -> Option<RepositoryInfo> {
    let repository = enclosing_repository(root, ancestors)?;
    Some(RepositoryInfo {
        vcs: Vcs::Git,
        is_workspace_root: platform::component_keys(&repository) == platform::component_keys(root),
        root: display_path(&repository).to_string_lossy().into_owned(),
    })
}

fn nested_repositories(root: &Path) -> (Vec<WorkspacePath>, bool) {
    let mut found = Vec::new();
    let mut inspected = 0usize;
    let mut queue = VecDeque::from([(root.to_path_buf(), Vec::<String>::new())]);
    while let Some((dir, relative)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<(String, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            inspected += 1;
            if inspected > MAX_SCAN_ENTRIES {
                found.sort_by(|a: &WorkspacePath, b| a.as_str().cmp(b.as_str()));
                return (found, true);
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name == ".git" {
                continue;
            }
            children.push((name, entry.path()));
        }
        children.sort();
        for (name, path) in children {
            let mut child = relative.clone();
            child.push(name);
            if is_repository_root(&path) {
                if let Ok(nested) = WorkspacePath::new(child.join("/")) {
                    found.push(nested);
                }
            }
            if child.len() < MAX_SCAN_DEPTH {
                queue.push_back((path, child));
            }
        }
    }
    found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    (found, false)
}
