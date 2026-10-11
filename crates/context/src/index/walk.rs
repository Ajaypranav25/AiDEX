//! The indexing walk: ignore rules, secret names, links, and size limits.

use std::time::UNIX_EPOCH;

use aidex_core::{CancelToken, WorkspacePath};
use aidex_workspace::{ConfinedRoot, parse_workspace_path};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::{ContextError, exclude};

/// Version-control and AiDEX metadata directories never indexed.
const RESERVED_DIRS: [&str; 4] = [".git", ".hg", ".svn", ".aidex"];

/// Files left out of the index, by reason. Ignored files are not counted: the
/// walker never visits them.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkipCounts {
    /// Credential-looking names (and everything under `.ssh`, `.aws`, ...).
    pub secret_name: u64,
    /// Files containing a private-key block.
    pub secret_content: u64,
    /// Symbolic links and junctions (never followed).
    pub link: u64,
    pub too_large: u64,
    /// NUL bytes or invalid UTF-8.
    pub binary: u64,
    /// Not portable workspace paths (or not Unicode).
    pub unsupported_name: u64,
    /// Unreadable entries, or files that changed or vanished while indexing.
    pub unreadable: u64,
}

impl SkipCounts {
    pub fn total(&self) -> u64 {
        self.secret_name
            + self.secret_content
            + self.link
            + self.too_large
            + self.binary
            + self.unsupported_name
            + self.unreadable
    }
}

/// A file the walk selected for indexing.
pub(crate) struct Candidate {
    pub path: WorkspacePath,
    pub size: u64,
    pub mtime_ns: i64,
}

pub(crate) struct WalkOutcome {
    pub files: Vec<Candidate>,
    pub truncated: bool,
}

/// Walk options taken from `IndexOptions`.
pub(crate) struct WalkSettings {
    pub max_file_bytes: u64,
    pub max_files: usize,
    pub global_excludes: bool,
}

fn is_reserved(name: &str) -> bool {
    RESERVED_DIRS
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
}

/// Modification time in nanoseconds since the Unix epoch (0 when unavailable).
pub(crate) fn mtime_ns(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_nanos()).unwrap_or(i64::MAX)
        })
}

/// Whether file metadata describes a symbolic link, junction, or other reparse point.
pub(crate) fn is_link(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// Collect indexable files beneath `root` in a deterministic (name) order.
///
/// `.gitignore`, `.ignore`, and `.git/info/exclude` apply (also outside a Git
/// repository); ignore files above the workspace root are not read, and the user's
/// global excludes file only when `global_excludes` is set. Hidden files are
/// included; reserved metadata directories and secret directories are pruned.
/// Links are counted and never followed.
pub(crate) fn collect(
    root: &ConfinedRoot,
    settings: &WalkSettings,
    skipped: &mut SkipCounts,
    cancel: &CancelToken,
) -> Result<Option<WalkOutcome>, ContextError> {
    let base = root.root().to_path_buf();
    std::fs::read_dir(&base)
        .map_err(|error| ContextError::io("cannot read the workspace root", error))?;
    let mut builder = WalkBuilder::new(&base);
    builder
        .hidden(false)
        .parents(false)
        .ignore(true)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(settings.global_excludes)
        .require_git(false)
        .ignore_case_insensitive(cfg!(windows))
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(|entry| {
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            let name = entry.file_name().to_string_lossy();
            !(entry.depth() > 0
                && is_dir
                && (is_reserved(&name) || exclude::is_secret_dir_name(&name)))
        });
    let mut files = Vec::new();
    let mut visited = 0usize;
    for result in builder.build() {
        visited += 1;
        if visited.is_multiple_of(256) && cancel.is_stopped() {
            return Ok(None);
        }
        let Ok(entry) = result else {
            skipped.unreadable += 1;
            continue;
        };
        if entry.depth() == 0 {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            skipped.unreadable += 1;
            continue;
        };
        if is_link(&metadata) {
            skipped.link += 1;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Some(path) = relative_path(&base, entry.path()) else {
            skipped.unsupported_name += 1;
            continue;
        };
        if exclude::is_secret_path(path.as_str()) {
            skipped.secret_name += 1;
            continue;
        }
        if metadata.len() > settings.max_file_bytes {
            skipped.too_large += 1;
            continue;
        }
        if files.len() >= settings.max_files {
            return Ok(Some(WalkOutcome {
                files,
                truncated: true,
            }));
        }
        files.push(Candidate {
            size: metadata.len(),
            mtime_ns: mtime_ns(&metadata),
            path,
        });
    }
    Ok(Some(WalkOutcome {
        files,
        truncated: false,
    }))
}

fn relative_path(base: &std::path::Path, absolute: &std::path::Path) -> Option<WorkspacePath> {
    let relative = absolute.strip_prefix(base).ok()?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_str()?.to_owned()),
            _ => return None,
        }
    }
    parse_workspace_path(&parts.join("/")).ok()
}
