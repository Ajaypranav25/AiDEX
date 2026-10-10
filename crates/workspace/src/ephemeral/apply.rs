//! Writing accepted ephemeral changes into the source workspace.
//!
//! Each file is re-verified immediately before it is written: the ephemeral content
//! must still hash to the reviewed value and the source must still match the
//! baseline (or already hold the result). Writes go to an exclusive temporary file
//! in the destination directory, are fsynced, and replace the target by rename.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use aidex_core::WorkspacePath;

use super::{
    baseline::{Baseline, FileState},
    review::{Entry, SourceState, entry, source_state},
    types::{ApplyFailure, ApplyReport, ChangeKind, FileChange},
};
use crate::{
    Access, ConfinedRoot, WorkspaceError, fsutil, platform::component_keys, walk::is_reserved_path,
};

enum Outcome {
    Applied,
    Conflict,
    Failed(String),
}

impl From<WorkspaceError> for Outcome {
    fn from(error: WorkspaceError) -> Self {
        match error {
            WorkspaceError::EscapesRoot(_)
            | WorkspaceError::SymlinkLeaf(_)
            | WorkspaceError::InvalidPath { .. } => Self::Conflict,
            other => Self::Failed(other.to_string()),
        }
    }
}

/// Apply `changes` (already selected by the caller) and fold every applied change
/// into `baseline`. Never touches paths outside `changes`.
pub(crate) fn apply(
    source: &ConfinedRoot,
    tree: &ConfinedRoot,
    changes: &[&FileChange],
    baseline: &mut Baseline,
) -> ApplyReport {
    let mut report = ApplyReport::default();
    for change in changes {
        let path = &change.path;
        let outcome = if change.conflict.is_some() || is_reserved_path(path) {
            Outcome::Conflict
        } else {
            let base = baseline.files.get(path.as_str()).cloned();
            match change.kind {
                ChangeKind::Added | ChangeKind::Modified => {
                    apply_write(source, tree, change, base.as_ref(), baseline)
                }
                ChangeKind::Deleted => apply_delete(source, path, base.as_ref(), baseline),
            }
        };
        match outcome {
            Outcome::Applied => report.applied.push(path.clone()),
            Outcome::Conflict => report.skipped_conflicts.push(path.clone()),
            Outcome::Failed(message) => report.failed.push(ApplyFailure {
                path: path.clone(),
                message,
            }),
        }
    }
    report
}

fn apply_write(
    source: &ConfinedRoot,
    tree: &ConfinedRoot,
    change: &FileChange,
    base: Option<&FileState>,
    baseline: &mut Baseline,
) -> Outcome {
    let path = &change.path;
    let (Some(size), Some(sha256)) = (change.size, change.sha256.clone()) else {
        return Outcome::Failed("the change has no content hash".into());
    };
    let new = FileState { size, sha256 };
    let ephemeral = match entry(tree, path) {
        Ok(Entry::File(file)) => file,
        Ok(_) => return Outcome::Conflict,
        Err(error) => return error.into(),
    };
    let bytes = match fs::read(&ephemeral) {
        Ok(bytes) => bytes,
        Err(error) => return Outcome::Failed(format!("cannot read ephemeral file: {error}")),
    };
    if bytes.len() as u64 != new.size || fsutil::sha256_hex(&bytes) != new.sha256 {
        // The ephemeral file changed after review.
        return Outcome::Conflict;
    }
    let current = match source_state(source, path) {
        Ok(current) => current,
        Err(error) => return error.into(),
    };
    if current.matches(Some(&new)) {
        baseline.files.insert(path.as_str().to_owned(), new);
        return Outcome::Applied;
    }
    if !current.matches(base) {
        return Outcome::Conflict;
    }
    let mode_from = match &current {
        SourceState::File { path, .. } => path.clone(),
        _ => ephemeral,
    };
    match write_confined(source, path, &bytes, &mode_from) {
        Ok(()) => {
            baseline.files.insert(path.as_str().to_owned(), new);
            Outcome::Applied
        }
        Err(outcome) => outcome,
    }
}

fn apply_delete(
    source: &ConfinedRoot,
    path: &WorkspacePath,
    base: Option<&FileState>,
    baseline: &mut Baseline,
) -> Outcome {
    let current = match source_state(source, path) {
        Ok(current) => current,
        Err(error) => return error.into(),
    };
    match current {
        SourceState::Missing => {
            baseline.files.remove(path.as_str());
            Outcome::Applied
        }
        SourceState::File {
            path: target,
            state,
        } if Some(&state) == base => match fs::remove_file(&target) {
            Ok(()) => {
                fsutil::sync_parent(&target);
                baseline.files.remove(path.as_str());
                Outcome::Applied
            }
            Err(error) => Outcome::Failed(format!("cannot delete: {error}")),
        },
        _ => Outcome::Conflict,
    }
}

fn same(a: &Path, b: &Path) -> bool {
    component_keys(a) == component_keys(b)
}

/// Atomically replace `path` in `source` with `bytes`, re-checking confinement
/// after creating parents and immediately before the rename.
fn write_confined(
    source: &ConfinedRoot,
    path: &WorkspacePath,
    bytes: &[u8],
    mode_from: &Path,
) -> Result<(), Outcome> {
    let target = source.resolve(path, Access::Create)?;
    let parent = target
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| Outcome::Failed("the target has no parent directory".into()))?;
    fs::create_dir_all(&parent)
        .map_err(|error| Outcome::Failed(format!("cannot create parent directories: {error}")))?;
    let checked: PathBuf = source.resolve(path, Access::Create)?;
    if !same(&checked, &target) {
        return Err(Outcome::Conflict);
    }
    let (temporary, mut file) = fsutil::sibling_temp(&checked)
        .map_err(|error| Outcome::Failed(format!("cannot create a temporary file: {error}")))?;
    let written = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fsutil::copy_unix_mode(mode_from, &temporary)
    })();
    let result = match written {
        Err(error) => Err(Outcome::Failed(format!("cannot write: {error}"))),
        Ok(()) => match source.resolve(path, Access::Create) {
            Ok(last) if same(&last, &checked) => fs::rename(&temporary, &checked)
                .map_err(|error| Outcome::Failed(format!("cannot replace the file: {error}"))),
            Ok(_) => Err(Outcome::Conflict),
            Err(error) => Err(error.into()),
        },
    };
    match &result {
        Ok(()) => fsutil::sync_parent(&checked),
        Err(_) => {
            let _ = fs::remove_file(&temporary);
        }
    }
    result
}
