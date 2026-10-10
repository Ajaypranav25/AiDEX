//! `recover`: bring the drive back to a valid state after a crash or power loss.
//!
//! Runs when the daemon starts (before it serves requests; the daemon is the drive's
//! only updater). It removes a leftover `app/update.lock`, temporary files of
//! interrupted atomic writes (`.current.json.*`, `.update-journal.json.*`), and the
//! updater's staging directories, then looks at the journal:
//!
//! | Journal state | Recovery |
//! | --- | --- |
//! | none, `committed`, `aborted` | nothing to undo |
//! | `staging`, `verified`, `self_test` | the pointer was never touched: roll back (restore `before` if it differs, remove staging) |
//! | `switching`, pointer selects `to` and `to` verifies | roll forward: place launchers, copy provenance, mark `committed`, prune |
//! | `switching`, anything else | roll back: restore the target's `before` entry (or the whole `before` pointer if `current.json` is unreadable), remove the version directory the operation created |
//! | unreadable journal | set it aside as `update-journal.json.corrupt-<ms>`; the pointer is left as is |
//!
//! In every case `app/current.json` ends up selecting either the version that was
//! active before the operation or the new, fully verified one.

use std::fs;

use aidex_bundle::{
    BundleError, CurrentPointer, JOURNAL_FILE_NAME, JournalOperation, JournalState,
    POINTER_FILE_NAME, UpdateJournal, VersionRef, pointer_dir, stale_temporaries,
};
use aidex_core::{EntityId, Timestamp};
use aidex_events::EventKind;
use serde::{Deserialize, Serialize};

use crate::{UpdateError, Updater, lock};

/// What recovery did.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcome {
    /// No unfinished operation (leftovers may still have been removed).
    Clean,
    /// An interrupted operation was undone; the previous bundle is active.
    RolledBack,
    /// An operation interrupted after its pointer switch was completed.
    RolledForward,
    /// The journal was unreadable and was set aside.
    JournalSetAside,
}

/// The result of [`Updater::recover`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryReport {
    pub outcome: RecoveryOutcome,
    pub update_id: Option<EntityId>,
    pub operation: Option<JournalOperation>,
    /// The step the interrupted operation was in.
    pub interrupted_state: Option<JournalState>,
    /// The bundle the pointer selects for the target afterwards.
    pub active: Option<VersionRef>,
    /// Whether `app/current.json` was rewritten to the pre-operation pointer.
    pub restored_pointer: bool,
    /// Leftovers removed (relative to `app/`).
    pub removed: Vec<String>,
    pub warnings: Vec<String>,
}

impl RecoveryReport {
    fn new() -> Self {
        Self {
            outcome: RecoveryOutcome::Clean,
            update_id: None,
            operation: None,
            interrupted_state: None,
            active: None,
            restored_pointer: false,
            removed: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

impl Updater {
    /// Recover from an interrupted update or rollback (see the module docs). Call it
    /// only where no other process can be updating the drive (daemon start).
    pub fn recover(&self) -> Result<RecoveryReport, UpdateError> {
        let _gate = self.gate()?;
        let result = self.run_recover();
        if let Err(error) = &result {
            self.emit_failed(None, "recover", error);
        }
        result
    }

    fn run_recover(&self) -> Result<RecoveryReport, UpdateError> {
        let app = self.app()?;
        let mut report = RecoveryReport::new();
        if let Some(holder) = lock::remove_stale(&app)? {
            report
                .removed
                .push(format!("{} ({holder})", lock::LOCK_FILE_NAME));
        }
        for name in [POINTER_FILE_NAME, JOURNAL_FILE_NAME] {
            let leftovers = stale_temporaries(&app, name).map_err(|error| {
                UpdateError::io(format!("cannot list `{}`", app.display()), error)
            })?;
            for path in leftovers {
                match fs::remove_file(&path) {
                    Ok(()) => report.removed.push(file_name(&path)),
                    Err(error) => report
                        .warnings
                        .push(format!("cannot remove `{}`: {error}", path.display())),
                }
            }
        }
        let journal_file = self.journal_file();
        let journal = match UpdateJournal::load(&journal_file) {
            Ok(journal) => journal,
            Err(BundleError::Journal { .. }) => {
                let aside = app.join(format!(
                    "{JOURNAL_FILE_NAME}.corrupt-{}",
                    Timestamp::now().unix_millis()
                ));
                fs::rename(&journal_file, &aside).map_err(|error| {
                    UpdateError::io(
                        format!("cannot set `{}` aside", journal_file.display()),
                        error,
                    )
                })?;
                report.outcome = RecoveryOutcome::JournalSetAside;
                report.warnings.push(format!(
                    "the update journal was unreadable and was kept as `{}`",
                    file_name(&aside)
                ));
                self.finish_clean(&mut report);
                return Ok(report);
            }
            Err(error) => return Err(error.into()),
        };
        let Some(mut journal) = journal.filter(|journal| !journal.state.is_terminal()) else {
            self.finish_clean(&mut report);
            return Ok(report);
        };
        report.update_id = Some(journal.id.clone());
        report.operation = Some(journal.operation);
        report.interrupted_state = Some(journal.state);
        if journal.state == JournalState::Switching && self.switch_completed(&journal) {
            self.roll_forward(&mut journal, &mut report)?;
        } else {
            self.roll_back(&mut journal, &mut report)?;
        }
        Ok(report)
    }

    fn finish_clean(&self, report: &mut RecoveryReport) {
        let removed = self.clean_staging(&mut report.warnings);
        report.removed.extend(removed);
        report.active = self
            .load_pointer()
            .ok()
            .flatten()
            .and_then(|pointer| pointer.get(self.target).map(|entry| entry.current()));
    }

    /// The pointer selects the journal's `to` bundle and that bundle verifies.
    fn switch_completed(&self, journal: &UpdateJournal) -> bool {
        let Some(to) = &journal.to else {
            return false;
        };
        let Ok(Some(pointer)) = self.load_pointer() else {
            return false;
        };
        let selected = pointer.get(self.target).is_some_and(|entry| {
            entry.dir == to.dir && entry.manifest_sha256 == to.manifest_sha256
        });
        if !selected {
            return false;
        }
        // An unsigned bundle was accepted only through the development opt-in; keeping
        // the pointer is harmless because the launcher still refuses it without one.
        let Ok(policy) = self.policy(journal.signer_key_id.is_none()) else {
            return false;
        };
        self.version_path(&to.dir).is_ok_and(|dir| {
            self.verify_dir(&dir, &policy, Some(&to.manifest_sha256), false)
                .is_ok()
        })
    }

    fn roll_forward(
        &self,
        journal: &mut UpdateJournal,
        report: &mut RecoveryReport,
    ) -> Result<(), UpdateError> {
        let to = journal.to.clone().ok_or(UpdateError::NotInstalled {
            target: self.target,
        })?;
        let dir = self.version_path(&to.dir)?;
        let policy = self.policy(journal.signer_key_id.is_none())?;
        let candidate = self.verify_dir(&dir, &policy, Some(&to.manifest_sha256), false)?;
        let manifest = candidate.manifest();
        self.place_launchers(&dir, manifest, &mut report.warnings);
        if journal.operation == JournalOperation::Apply
            && let Ok(name) = to.dir_name()
        {
            self.copy_provenance(&dir, name, &mut report.warnings);
        }
        journal.finish(
            JournalState::Committed,
            format!(
                "recovered: completed the interrupted {} to {}",
                journal.operation.as_str(),
                to.version
            ),
        );
        journal.store(&self.journal_file())?;
        let from_version = journal
            .before
            .as_ref()
            .and_then(|before| before.get(self.target))
            .map(|entry| entry.version.clone());
        match journal.operation {
            JournalOperation::Apply => self.emit(EventKind::UpdateSwitched {
                update_id: journal.id.clone(),
                operation: JournalOperation::Apply.as_str().to_owned(),
                from_version,
                to_version: to.version.clone(),
            }),
            JournalOperation::Rollback => self.emit(EventKind::UpdateRolledBack {
                update_id: journal.id.clone(),
                from_version,
                to_version: Some(to.version.clone()),
                reason: "requested (completed by recovery)".into(),
            }),
        }
        let removed = self.clean_staging(&mut report.warnings);
        report.removed.extend(removed);
        if journal.operation == JournalOperation::Apply
            && let Ok(Some(pointer)) = self.load_pointer()
        {
            let pruned = self.prune(&pointer, journal.id.as_str(), &mut report.warnings);
            report
                .removed
                .extend(pruned.into_iter().map(|name| format!("versions/{name}")));
        }
        report.outcome = RecoveryOutcome::RolledForward;
        report.active = Some(to);
        Ok(())
    }

    fn roll_back(
        &self,
        journal: &mut UpdateJournal,
        report: &mut RecoveryReport,
    ) -> Result<(), UpdateError> {
        let target_key = self.target.as_str().to_owned();
        let before_entry = journal
            .before
            .as_ref()
            .and_then(|before| before.get(self.target))
            .cloned();
        let pointer_file = self.pointer_file();
        let restored = match CurrentPointer::load(&pointer_file) {
            Ok(Some(mut pointer)) => {
                if pointer.get(self.target) != before_entry.as_ref() {
                    match &before_entry {
                        Some(entry) => {
                            pointer.targets.insert(target_key, entry.clone());
                        }
                        None => {
                            pointer.targets.remove(&target_key);
                        }
                    }
                    pointer.store(&pointer_file)?;
                    report.restored_pointer = true;
                }
                Some(pointer)
            }
            Ok(None) => match &journal.before {
                Some(before) => {
                    before.store(&pointer_file)?;
                    report.restored_pointer = true;
                    Some(before.clone())
                }
                None => None,
            },
            Err(_) => {
                // current.json is unreadable (a filesystem that tore the rename):
                // restore the whole pointer recorded before the operation.
                match &journal.before {
                    Some(before) => before.store(&pointer_file)?,
                    None => fs::remove_file(&pointer_file).map_err(|error| {
                        UpdateError::io(
                            format!("cannot remove `{}`", pointer_file.display()),
                            error,
                        )
                    })?,
                }
                report.restored_pointer = true;
                journal.before.clone()
            }
        };
        if journal.created_version_dir
            && let Some(name) = journal
                .to
                .as_ref()
                .and_then(|to| to.dir_name().ok().map(str::to_owned))
        {
            let dir = pointer_dir(&name);
            let referenced = restored.as_ref().is_some_and(|pointer| {
                pointer.targets.values().any(|entry| {
                    entry.dir == dir
                        || entry
                            .previous
                            .as_ref()
                            .is_some_and(|previous| previous.dir == dir)
                })
            });
            if !referenced {
                match self.remove_version_dir(&name, journal.id.as_str()) {
                    Ok(()) => report.removed.push(format!("versions/{name}")),
                    Err(error) => report.warnings.push(error.to_string()),
                }
            }
        }
        let removed = self.clean_staging(&mut report.warnings);
        report.removed.extend(removed);
        let interrupted = journal.state;
        journal.finish(
            JournalState::Aborted,
            format!(
                "recovered: rolled back the {} interrupted during `{}`",
                journal.operation.as_str(),
                interrupted.as_str()
            ),
        );
        journal.store(&self.journal_file())?;
        self.emit(EventKind::UpdateRolledBack {
            update_id: journal.id.clone(),
            from_version: journal
                .to
                .as_ref()
                .map(|to| to.version.clone())
                .or_else(|| journal.candidate_version.clone()),
            to_version: before_entry.as_ref().map(|entry| entry.version.clone()),
            reason: format!("interrupted during {}", interrupted.as_str()),
        });
        report.outcome = RecoveryOutcome::RolledBack;
        report.active = before_entry.map(|entry| entry.current());
        Ok(())
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
