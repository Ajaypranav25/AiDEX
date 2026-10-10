//! Computing the reviewable diff of an ephemeral workspace against its baseline,
//! with conflict detection against the current source.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use aidex_core::{EntityId, WorkspacePath};
use sha2::{Digest, Sha256};
use similar::TextDiff;

use super::{
    baseline::{Baseline, FileState},
    types::{ChangeKind, ConflictReason, EphemeralDiff, EphemeralLimits, FileChange},
};
use crate::{
    Access, ConfinedRoot, SkipReason, SkippedEntry, WorkspaceError, fsutil,
    platform::component_keys,
    walk::{self, Item},
};

/// What a confined root holds at a workspace path, without following links.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Entry {
    Missing,
    File(PathBuf),
    /// The path or one of its components is a link (inside or outside the root).
    Link,
    /// A directory, device, or other non-file.
    Other,
}

/// Inspect `path` under `root`, refusing to look through any link.
pub(crate) fn entry(root: &ConfinedRoot, path: &WorkspacePath) -> Result<Entry, WorkspaceError> {
    let resolved = match root.resolve(path, Access::Read) {
        Ok(resolved) => resolved,
        Err(WorkspaceError::PathNotFound(_)) => return Ok(Entry::Missing),
        Err(WorkspaceError::EscapesRoot(_) | WorkspaceError::SymlinkLeaf(_)) => {
            return Ok(Entry::Link);
        }
        Err(WorkspaceError::InvalidPath { .. }) => return Ok(Entry::Other),
        Err(error) => return Err(error),
    };
    let mut lexical = root.root().to_path_buf();
    lexical.extend(path.as_str().split('/'));
    if component_keys(&resolved) != component_keys(&lexical) || fsutil::is_link(&lexical) {
        return Ok(Entry::Link);
    }
    let metadata = fs::symlink_metadata(&resolved).map_err(|error| {
        WorkspaceError::io(format!("cannot inspect `{}`", path.as_str()), error)
    })?;
    Ok(if metadata.is_file() {
        Entry::File(resolved)
    } else {
        Entry::Other
    })
}

/// The source's state at a path, for conflict decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SourceState {
    Missing,
    File { path: PathBuf, state: FileState },
    Unsafe,
    Other,
}

impl SourceState {
    /// Whether the source matches `expected` (`None` = absent).
    pub(crate) fn matches(&self, expected: Option<&FileState>) -> bool {
        match (self, expected) {
            (Self::Missing, None) => true,
            (Self::File { state, .. }, Some(expected)) => state == expected,
            _ => false,
        }
    }
}

pub(crate) fn source_state(
    source: &ConfinedRoot,
    path: &WorkspacePath,
) -> Result<SourceState, WorkspaceError> {
    Ok(match entry(source, path)? {
        Entry::Missing => SourceState::Missing,
        Entry::Link => SourceState::Unsafe,
        Entry::Other => SourceState::Other,
        Entry::File(resolved) => {
            let (size, sha256) = fsutil::hash_file(&resolved).map_err(|error| {
                WorkspaceError::io(format!("cannot read source `{}`", path.as_str()), error)
            })?;
            SourceState::File {
                path: resolved,
                state: FileState { size, sha256 },
            }
        }
    })
}

/// Conflict rule: the source may be applied over only when it still matches the
/// baseline, or already matches the result.
pub(crate) fn conflict(
    base: Option<&FileState>,
    new: Option<&FileState>,
    source: &SourceState,
) -> Option<ConflictReason> {
    if *source == SourceState::Unsafe {
        return Some(ConflictReason::UnsafePath);
    }
    if source.matches(new) || source.matches(base) {
        return None;
    }
    Some(match (source, base) {
        (SourceState::Missing, _) => ConflictReason::SourceRemoved,
        (_, None) => ConflictReason::SourceAdded,
        (SourceState::File { .. }, Some(_)) => ConflictReason::SourceChanged,
        _ => ConflictReason::SourceRemoved,
    })
}

/// A detected change before conflict and diff computation.
struct Pending {
    path: WorkspacePath,
    kind: ChangeKind,
    base: Option<FileState>,
    new: Option<FileState>,
    /// The ephemeral file holding the new content.
    ephemeral: Option<PathBuf>,
}

pub(crate) struct ReviewInput<'a> {
    pub workspace_id: &'a EntityId,
    pub source_id: &'a EntityId,
    pub source: &'a ConfinedRoot,
    pub tree: &'a ConfinedRoot,
    pub baseline: &'a Baseline,
    pub limits: &'a EphemeralLimits,
}

pub(crate) fn review(input: &ReviewInput<'_>) -> Result<EphemeralDiff, WorkspaceError> {
    let mut walked: BTreeMap<String, (WorkspacePath, PathBuf)> = BTreeMap::new();
    let mut unsupported: Vec<SkippedEntry> = Vec::new();
    walk::walk(input.tree.root(), Vec::new(), |item| {
        match item {
            Item::File { path, absolute, .. } => {
                walked.insert(path.as_str().to_owned(), (path, absolute));
            }
            Item::Skipped(skipped) => unsupported.push(skipped),
            Item::Dir { .. } => {}
        }
        Ok(())
    })?;
    let mut flagged: BTreeSet<String> =
        unsupported.iter().map(|entry| entry.path.clone()).collect();
    let mut flag =
        |path: &WorkspacePath, reason: SkipReason, unsupported: &mut Vec<SkippedEntry>| {
            if flagged.insert(path.as_str().to_owned()) {
                unsupported.push(SkippedEntry {
                    path: path.as_str().to_owned(),
                    reason,
                });
            }
        };

    let mut pending: Vec<Pending> = Vec::new();
    for (key, base) in &input.baseline.files {
        let path = WorkspacePath::new(key.as_str())
            .map_err(|_| WorkspaceError::Corrupt(format!("baseline path `{key}`")))?;
        walked.remove(key);
        match entry(input.tree, &path)? {
            Entry::Missing => pending.push(Pending {
                path,
                kind: ChangeKind::Deleted,
                base: Some(base.clone()),
                new: None,
                ephemeral: None,
            }),
            Entry::Link => flag(&path, SkipReason::Link, &mut unsupported),
            Entry::Other => flag(&path, SkipReason::NotAFile, &mut unsupported),
            Entry::File(absolute) => {
                let state = hash(&absolute, &path)?;
                if state != *base {
                    pending.push(Pending {
                        path,
                        kind: ChangeKind::Modified,
                        base: Some(base.clone()),
                        new: Some(state),
                        ephemeral: Some(absolute),
                    });
                }
            }
        }
    }
    for (_, (path, absolute)) in walked {
        if walk::is_reserved_path(&path) {
            continue;
        }
        let state = hash(&absolute, &path)?;
        pending.push(Pending {
            path,
            kind: ChangeKind::Added,
            base: None,
            new: Some(state),
            ephemeral: Some(absolute),
        });
    }
    pending.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));
    unsupported.sort_by(|a, b| a.path.cmp(&b.path));

    let mut budget = input.limits.max_total_diff_bytes;
    let mut changes = Vec::with_capacity(pending.len());
    for Pending {
        path,
        kind,
        base,
        new,
        ephemeral,
    } in pending
    {
        let source = source_state(input.source, &path)?;
        let conflict = conflict(base.as_ref(), new.as_ref(), &source);
        let old_file = match &source {
            SourceState::File { path, .. } => Some(path.as_path()),
            _ => None,
        };
        let text = diff_text(
            &path,
            old_file,
            ephemeral.as_deref(),
            input.limits,
            &mut budget,
        )?;
        changes.push(FileChange {
            path,
            kind,
            binary: text.binary,
            size: new.as_ref().map(|state| state.size),
            sha256: new.map(|state| state.sha256),
            unified_diff: text.diff,
            diff_truncated: text.truncated,
            conflict,
        });
    }
    let conflicts = changes
        .iter()
        .filter(|change| change.conflict.is_some())
        .map(|change| change.path.clone())
        .collect();
    Ok(EphemeralDiff {
        workspace_id: input.workspace_id.clone(),
        source_id: input.source_id.clone(),
        digest: digest(&changes),
        changes,
        conflicts,
        unsupported,
    })
}

fn hash(absolute: &Path, path: &WorkspacePath) -> Result<FileState, WorkspaceError> {
    let (size, sha256) = fsutil::hash_file(absolute).map_err(|error| {
        WorkspaceError::io(format!("cannot read ephemeral `{}`", path.as_str()), error)
    })?;
    Ok(FileState { size, sha256 })
}

/// SHA-256 over the canonical change list; paths cannot contain tabs or newlines.
pub(crate) fn digest(changes: &[FileChange]) -> String {
    let mut hasher = Sha256::new();
    for change in changes {
        hasher.update(
            format!(
                "{}\t{}\t{}\t{}\n",
                change.kind.label(),
                change.path.as_str(),
                change.sha256.as_deref().unwrap_or("-"),
                change.conflict.map_or("-", ConflictReason::label),
            )
            .as_bytes(),
        );
    }
    fsutil::hex(&hasher.finalize())
}

struct DiffText {
    binary: bool,
    diff: Option<String>,
    truncated: bool,
}

const SNIFF_BYTES: usize = 8 * 1024;

fn sniff_binary(path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let mut buffer = vec![0u8; SNIFF_BYTES];
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let read = file.read(&mut buffer).unwrap_or(0);
    buffer[..read].contains(&0)
}

fn read_side(
    path: Option<&Path>,
    workspace_path: &WorkspacePath,
) -> Result<Vec<u8>, WorkspaceError> {
    match path {
        None => Ok(Vec::new()),
        Some(path) => fs::read(path).map_err(|error| {
            WorkspaceError::io(format!("cannot read `{}`", workspace_path.as_str()), error)
        }),
    }
}

fn diff_text(
    path: &WorkspacePath,
    old: Option<&Path>,
    new: Option<&Path>,
    limits: &EphemeralLimits,
    budget: &mut usize,
) -> Result<DiffText, WorkspaceError> {
    let too_large = [old, new].into_iter().flatten().any(|side| {
        fs::metadata(side).map_or(true, |metadata| {
            metadata.len() > limits.max_diff_input_bytes
        })
    });
    if too_large {
        return Ok(DiffText {
            binary: sniff_binary(old) || sniff_binary(new),
            diff: None,
            truncated: true,
        });
    }
    let old_bytes = read_side(old, path)?;
    let new_bytes = read_side(new, path)?;
    let (Ok(old_text), Ok(new_text)) = (
        std::str::from_utf8(&old_bytes),
        std::str::from_utf8(&new_bytes),
    ) else {
        return Ok(DiffText {
            binary: true,
            diff: None,
            truncated: false,
        });
    };
    if old_text.contains('\0') || new_text.contains('\0') {
        return Ok(DiffText {
            binary: true,
            diff: None,
            truncated: false,
        });
    }
    if *budget == 0 {
        return Ok(DiffText {
            binary: false,
            diff: None,
            truncated: true,
        });
    }
    let old_header = old.map_or_else(
        || "/dev/null".to_owned(),
        |_| format!("a/{}", path.as_str()),
    );
    let new_header = new.map_or_else(
        || "/dev/null".to_owned(),
        |_| format!("b/{}", path.as_str()),
    );
    let mut text = TextDiff::from_lines(old_text, new_text)
        .unified_diff()
        .context_radius(3)
        .header(&old_header, &new_header)
        .to_string();
    let cap = limits.max_file_diff_bytes.min(*budget);
    let mut truncated = false;
    if text.len() > cap {
        let mut cut = cap;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        let cut = text[..cut].rfind('\n').map_or(0, |index| index + 1);
        text.truncate(cut);
        truncated = true;
    }
    *budget -= text.len();
    Ok(DiffText {
        binary: false,
        diff: Some(text),
        truncated,
    })
}
