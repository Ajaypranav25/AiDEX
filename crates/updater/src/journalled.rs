//! Journal transitions with checkpoint hooks, and the crash-safe filesystem steps the
//! operations share (pointer writes, staging and version-directory removal).

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use aidex_bundle::{
    BundleError, CurrentPointer, InstallStep, JournalState, UPDATE_ID_PREFIX, UpdateJournal,
    layout::STAGING_DIR,
};

use crate::{Checkpoint, Hook, UpdateError, Updater};

/// The journal of a running operation and its checkpoint hook.
pub(crate) struct Journalled<'h> {
    pub journal: UpdateJournal,
    path: PathBuf,
    hook: Hook<'h>,
}

impl<'h> Journalled<'h> {
    pub fn new(journal: UpdateJournal, path: PathBuf, hook: Hook<'h>) -> Self {
        Self {
            journal,
            path,
            hook,
        }
    }

    /// Record `state`, then pass `checkpoint`.
    pub fn enter(
        &mut self,
        state: JournalState,
        checkpoint: Checkpoint,
    ) -> Result<(), UpdateError> {
        self.journal.advance(state);
        self.journal.store(&self.path)?;
        self.checkpoint(checkpoint)
    }

    /// Give the hook a chance to simulate a crash here.
    pub fn checkpoint(&mut self, checkpoint: Checkpoint) -> Result<(), UpdateError> {
        (self.hook)(checkpoint).map_err(|_| UpdateError::Interrupted(checkpoint))
    }

    /// Finish in a terminal state.
    pub fn finish(
        &mut self,
        state: JournalState,
        outcome: impl Into<String>,
    ) -> Result<(), UpdateError> {
        self.journal.finish(state, outcome);
        self.journal.store(&self.path)?;
        Ok(())
    }

    /// Replace `app/current.json` with `pointer`, passing [`Checkpoint::PointerWritten`]
    /// between the synced temporary file and the rename, and
    /// [`Checkpoint::PointerSwitched`] after it.
    pub fn write_pointer(
        &mut self,
        pointer: &CurrentPointer,
        pointer_file: &Path,
    ) -> Result<(), UpdateError> {
        let mut interrupted = false;
        let hook = &mut self.hook;
        let result = pointer.store_with(pointer_file, &mut |_temporary| {
            hook(Checkpoint::PointerWritten).map_err(|_| {
                interrupted = true;
                BundleError::Aborted(InstallStep::SwitchPointer)
            })
        });
        if interrupted {
            return Err(UpdateError::Interrupted(Checkpoint::PointerWritten));
        }
        result?;
        self.checkpoint(Checkpoint::PointerSwitched)
    }
}

impl Updater {
    pub(crate) fn staging_root(&self) -> Result<PathBuf, UpdateError> {
        let app = self.app()?;
        Ok(self.paths.ensure_dir(&app.join(STAGING_DIR))?)
    }

    /// Remove every updater-owned (`upd_*`) entry of `app/staging/`; returns their names.
    pub(crate) fn clean_staging(&self, warnings: &mut Vec<String>) -> Vec<String> {
        let root = self.paths.app().join(STAGING_DIR);
        let prefix = format!("{UPDATE_ID_PREFIX}_");
        let mut removed = Vec::new();
        let Ok(entries) = fs::read_dir(&root) else {
            return removed;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .filter(|name| name.starts_with(&prefix))
            .collect();
        names.sort();
        for name in names {
            match remove_any(&root.join(&name)) {
                Ok(()) => removed.push(format!("{STAGING_DIR}/{name}")),
                Err(error) => {
                    warnings.push(format!("cannot remove `{STAGING_DIR}/{name}`: {error}"))
                }
            }
        }
        removed
    }

    /// Remove `app/versions/<name>` by first renaming it into staging (so a crash in
    /// the middle leaves only staging debris, which recovery removes).
    pub(crate) fn remove_version_dir(&self, name: &str, tag: &str) -> Result<(), UpdateError> {
        let source = aidex_bundle::layout::versions_dir(&self.paths).join(name);
        let parked = self
            .staging_root()?
            .join(format!("{UPDATE_ID_PREFIX}_{tag}.removed-{name}"));
        match fs::rename(&source, &parked) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(UpdateError::io(
                    format!("cannot move `{}` aside", source.display()),
                    error,
                ));
            }
        }
        aidex_bundle::sync_dir(&aidex_bundle::layout::versions_dir(&self.paths));
        remove_any(&parked).map_err(|error| {
            UpdateError::io(format!("cannot remove `{}`", parked.display()), error)
        })
    }
}

/// Remove a file or a directory tree (links are removed, never followed).
pub(crate) fn remove_any(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)
        }
        Ok(_) => fs::remove_file(path).or_else(|error| {
            // A directory symlink/junction on Windows is removed with remove_dir.
            fs::remove_dir(path).map_err(|_| error)
        }),
    }
}
