//! Copying a source workspace into an ephemeral tree and recording its baseline.

use std::{collections::BTreeMap, fs, path::Path};

use super::{baseline::FileState, types::EphemeralLimits};
use crate::{
    ConfinedRoot, SkippedEntry, WorkspaceError, fsutil, platform,
    walk::{self, Item},
};

pub(crate) struct CopyOutcome {
    pub files: BTreeMap<String, FileState>,
    pub bytes: u64,
    pub skipped: Vec<SkippedEntry>,
}

/// Copy every non-ignored regular file of `source` into the empty directory
/// `destination`, hashing exactly the bytes written. Links are never followed or
/// reproduced; `.git` and `.aidex` are skipped. Exceeding a limit aborts the copy.
pub(crate) fn copy_tree(
    source: &ConfinedRoot,
    destination: &Path,
    limits: &EphemeralLimits,
) -> Result<CopyOutcome, WorkspaceError> {
    let mut outcome = CopyOutcome {
        files: BTreeMap::new(),
        bytes: 0,
        skipped: Vec::new(),
    };
    let inherited = walk::inherited_matchers(source.root());
    walk::walk(source.root(), inherited, |item| {
        match item {
            Item::Dir { absolute } => {
                let target = target_for(source, destination, &absolute)?;
                fs::create_dir(&target).map_err(|error| {
                    WorkspaceError::io(format!("cannot create `{}`", target.display()), error)
                })?;
            }
            Item::File {
                path,
                absolute,
                len,
            } => {
                let count = outcome.files.len() as u64 + 1;
                if count > limits.max_files {
                    return Err(WorkspaceError::TooLarge(format!(
                        "the source has more than {} files; the ephemeral copy was discarded",
                        limits.max_files
                    )));
                }
                if outcome.bytes.saturating_add(len) > limits.max_bytes {
                    return Err(WorkspaceError::TooLarge(format!(
                        "the source exceeds {} bytes; the ephemeral copy was discarded",
                        limits.max_bytes
                    )));
                }
                let target = target_for(source, destination, &absolute)?;
                let (size, sha256) = fsutil::copy_hashed(&absolute, &target).map_err(|error| {
                    WorkspaceError::io(format!("cannot copy `{}`", path.as_str()), error)
                })?;
                outcome.bytes = outcome.bytes.saturating_add(size);
                if outcome.bytes > limits.max_bytes {
                    return Err(WorkspaceError::TooLarge(format!(
                        "the source exceeds {} bytes; the ephemeral copy was discarded",
                        limits.max_bytes
                    )));
                }
                outcome
                    .files
                    .insert(path.as_str().to_owned(), FileState { size, sha256 });
            }
            Item::Skipped(skipped) => outcome.skipped.push(skipped),
        }
        Ok(())
    })?;
    Ok(outcome)
}

fn target_for(
    source: &ConfinedRoot,
    destination: &Path,
    absolute: &Path,
) -> Result<std::path::PathBuf, WorkspaceError> {
    let relative = platform::strip_root(absolute, source.root())
        .ok_or_else(|| WorkspaceError::EscapesRoot(absolute.display().to_string()))?;
    let mut target = destination.to_path_buf();
    target.extend(relative);
    Ok(target)
}
