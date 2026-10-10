//! Repository metadata read from the local `.git` directory (never the network).
//!
//! Only the current branch, the HEAD commit id, and remote *names* are read.
//! Remote URLs are deliberately skipped: they can embed credentials.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use aidex_workspace::{RepositoryInfo, Vcs};
use serde::{Deserialize, Serialize};

/// Largest `.git` metadata file read (HEAD, config, refs).
const MAX_META_BYTES: u64 = 256 * 1024;
/// Largest `packed-refs` read.
const MAX_PACKED_REFS_BYTES: u64 = 8 * 1024 * 1024;
const MAX_REMOTES: usize = 32;

/// Version-control facts recorded with the index.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryMetadata {
    pub vcs: Option<Vcs>,
    /// Whether the workspace root is the repository root.
    pub is_workspace_root: bool,
    /// Current branch (`None` when HEAD is detached or unreadable).
    pub branch: Option<String>,
    /// Commit id HEAD points at, when it can be resolved locally.
    pub head: Option<String>,
    pub detached: bool,
    /// Remote names from the repository config (URLs are never read).
    pub remotes: Vec<String>,
}

/// Read metadata for the repository containing a workspace, if any.
pub(crate) fn read(info: Option<&RepositoryInfo>) -> Option<RepositoryMetadata> {
    let info = info?;
    let mut metadata = RepositoryMetadata {
        vcs: Some(info.vcs),
        is_workspace_root: info.is_workspace_root,
        ..RepositoryMetadata::default()
    };
    if info.vcs != Vcs::Git {
        return Some(metadata);
    }
    let Some((git_dir, common_dir)) = git_dirs(Path::new(&info.root)) else {
        return Some(metadata);
    };
    if let Some(head) = read_small(&git_dir.join("HEAD")) {
        let head = head.trim();
        if let Some(reference) = head.strip_prefix("ref:") {
            let reference = reference.trim();
            metadata.branch = reference
                .strip_prefix("refs/heads/")
                .filter(|branch| is_plain(branch, 256))
                .map(str::to_owned);
            metadata.head = resolve_ref(&git_dir, &common_dir, reference);
        } else if is_object_id(head) {
            metadata.detached = true;
            metadata.head = Some(head.to_owned());
        }
    }
    if let Some(config) = read_small(&common_dir.join("config")) {
        metadata.remotes = remote_names(&config);
    }
    Some(metadata)
}

/// The repository's git directory and common directory (they differ for linked
/// worktrees). A symlinked `.git` is not followed.
fn git_dirs(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let dot_git = root.join(".git");
    let metadata = fs::symlink_metadata(&dot_git).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }
    let git_dir = if metadata.is_dir() {
        dot_git
    } else {
        let pointer = read_small(&dot_git)?;
        let target = pointer.trim().strip_prefix("gitdir:")?.trim();
        let target = Path::new(target);
        if target.is_absolute() {
            target.to_path_buf()
        } else {
            root.join(target)
        }
    };
    let common = read_small(&git_dir.join("commondir"))
        .map(|text| {
            let text = text.trim().to_owned();
            let path = Path::new(&text);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                git_dir.join(path)
            }
        })
        .unwrap_or_else(|| git_dir.clone());
    Some((git_dir, common))
}

fn resolve_ref(git_dir: &Path, common_dir: &Path, reference: &str) -> Option<String> {
    if !reference.starts_with("refs/")
        || reference
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    for dir in [git_dir, common_dir] {
        let loose = reference
            .split('/')
            .fold(dir.to_path_buf(), |path, part| path.join(part));
        if let Some(id) = read_small(&loose) {
            let id = id.trim();
            if is_object_id(id) {
                return Some(id.to_owned());
            }
        }
    }
    let packed = read_bounded(&common_dir.join("packed-refs"), MAX_PACKED_REFS_BYTES)?;
    packed.lines().find_map(|line| {
        let (id, name) = line.split_once(' ')?;
        (name.trim() == reference && is_object_id(id)).then(|| id.to_owned())
    })
}

fn remote_names(config: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in config.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("[remote \"") else {
            continue;
        };
        let Some(name) = rest.strip_suffix("\"]") else {
            continue;
        };
        if is_plain(name, 64) && !names.iter().any(|known| known == name) {
            names.push(name.to_owned());
        }
        if names.len() >= MAX_REMOTES {
            break;
        }
    }
    names
}

fn is_plain(text: &str, max: usize) -> bool {
    !text.is_empty()
        && text.len() <= max
        && !text
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || ch == '@' || ch == ':')
}

fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn read_small(path: &Path) -> Option<String> {
    read_bounded(path, MAX_META_BYTES)
}

fn read_bounded(path: &Path, max: u64) -> Option<String> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > max {
        return None;
    }
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(max)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        dir
    }

    fn info(dir: &Path) -> RepositoryInfo {
        RepositoryInfo {
            vcs: Vcs::Git,
            root: dir.display().to_string(),
            is_workspace_root: true,
        }
    }

    #[test]
    fn branch_head_and_remote_names_are_read_without_urls() {
        let id = "a".repeat(40);
        let dir = repo(&[
            (".git/HEAD", "ref: refs/heads/main\n"),
            (".git/refs/heads/main", &format!("{id}\n")),
            (
                ".git/config",
                "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = https://user:pw@example.com/x.git\n[remote \"upstream\"]\n",
            ),
        ]);
        let metadata = read(Some(&info(dir.path()))).unwrap();
        assert_eq!(metadata.branch.as_deref(), Some("main"));
        assert_eq!(metadata.head.as_deref(), Some(id.as_str()));
        assert!(!metadata.detached);
        assert_eq!(metadata.remotes, ["origin", "upstream"]);
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(!json.contains("example.com"));
    }

    #[test]
    fn packed_refs_and_detached_heads_resolve() {
        let id = "b".repeat(40);
        let dir = repo(&[
            (".git/HEAD", "ref: refs/heads/dev\n"),
            (
                ".git/packed-refs",
                &format!("# pack-refs with: peeled\n{id} refs/heads/dev\n"),
            ),
        ]);
        assert_eq!(
            read(Some(&info(dir.path()))).unwrap().head.as_deref(),
            Some(id.as_str())
        );
        let detached = repo(&[(".git/HEAD", &format!("{id}\n"))]);
        let metadata = read(Some(&info(detached.path()))).unwrap();
        assert!(metadata.detached);
        assert_eq!(metadata.branch, None);
        assert!(read(None).is_none());
        let traversal = repo(&[(".git/HEAD", "ref: refs/../../outside\n")]);
        assert_eq!(read(Some(&info(traversal.path()))).unwrap().head, None);
    }
}
