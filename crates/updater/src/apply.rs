//! `check` and `apply`: stage → verify → self-test → switch → commit.

use std::{fs, path::Path};

use aidex_bundle::{
    CurrentPointer, JournalOperation, JournalState, LauncherPlacement, SignatureState,
    TargetPointer, UPDATE_ID_PREFIX, UpdateJournal, VersionRef, layout, pointer_dir, steps,
};
use aidex_core::EntityId;
use aidex_events::EventKind;
use serde::{Deserialize, Serialize};

use crate::{
    ApplyOptions, CandidateInfo, Checkpoint, ChecksumStatus, Hook, SelfTestReport, UpdateAction,
    UpdateError, UpdateSource, Updater,
    journalled::{Journalled, remove_any},
    lock::DriveLock,
    service::Candidate,
    source::{self, ResolvedSource},
    version::decide,
};

/// What [`Updater::check`] found, without changing anything.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CheckReport {
    pub source: UpdateSource,
    /// The package that was checked (an archive, or an unpacked bundle directory).
    pub package: String,
    pub checksum: ChecksumStatus,
    pub candidate: CandidateInfo,
    pub current: Option<VersionRef>,
    pub previous: Option<VersionRef>,
    /// What applying the package would do.
    pub action: UpdateAction,
}

/// What [`Updater::apply`] did.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplyReport {
    pub update_id: EntityId,
    pub action: UpdateAction,
    pub checksum: ChecksumStatus,
    pub candidate: CandidateInfo,
    /// The bundle that was active before.
    pub from: Option<VersionRef>,
    /// The bundle `app/current.json` now selects.
    pub to: VersionRef,
    pub self_test: Option<SelfTestReport>,
    pub launchers: Vec<LauncherPlacement>,
    /// Version directories removed after the commit.
    pub pruned: Vec<String>,
    pub warnings: Vec<String>,
    /// Running processes keep the old binaries; the new version starts on next launch.
    pub restart_required: bool,
}

/// A package staged and verified in `app/staging/<id>/`.
struct Staged {
    resolved: ResolvedSource,
    dir: std::path::PathBuf,
    candidate: Candidate,
}

impl Updater {
    /// Stage and verify `source` and report what applying it would do. Nothing is
    /// switched; the staging directory is removed.
    pub fn check(
        &self,
        source: &UpdateSource,
        options: &ApplyOptions,
    ) -> Result<CheckReport, UpdateError> {
        let _gate = self.gate()?;
        let id = EntityId::generate(UPDATE_ID_PREFIX).map_err(|error| {
            UpdateError::io(
                "cannot name the check",
                std::io::Error::other(error.to_string()),
            )
        })?;
        let staging = self.staging_root()?.join(id.as_str());
        let result = (|| {
            let staged = self.stage_and_verify(source, options, &staging)?;
            let pointer = self.load_pointer()?;
            let current = pointer.as_ref().and_then(|p| p.get(self.target)).cloned();
            let action = self.decide(current.as_ref(), &staged.candidate, options)?;
            Ok(CheckReport {
                source: source.clone(),
                package: staged.resolved.package.display().to_string(),
                checksum: staged.resolved.checksum,
                candidate: staged.candidate.info(),
                current: current.as_ref().map(TargetPointer::current),
                previous: current.and_then(|entry| entry.previous),
                action,
            })
        })();
        let _ = remove_any(&staging);
        if let Err(error) = &result {
            self.emit_failed(Some(&id), "check", error);
        }
        result
    }

    /// Apply `source`: stage, verify, self-test, switch `app/current.json`, commit,
    /// and prune. Every step is journalled; see the crate documentation.
    pub fn apply(
        &self,
        source: &UpdateSource,
        options: &ApplyOptions,
    ) -> Result<ApplyReport, UpdateError> {
        self.apply_with(source, options, &mut |_| Ok(()))
    }

    /// [`apply`](Self::apply) with a checkpoint hook (fault injection / progress).
    pub fn apply_with(
        &self,
        source: &UpdateSource,
        options: &ApplyOptions,
        hook: Hook<'_>,
    ) -> Result<ApplyReport, UpdateError> {
        let _gate = self.gate()?;
        self.ensure_no_pending()?;
        let app = self.app()?;
        let id = EntityId::generate(UPDATE_ID_PREFIX).map_err(|error| {
            UpdateError::io(
                "cannot name the update",
                std::io::Error::other(error.to_string()),
            )
        })?;
        let lock = DriveLock::acquire(&app, &id)?;
        let before = match self.load_pointer() {
            Ok(before) => before,
            Err(error) => {
                lock.release();
                return Err(error);
            }
        };
        let journal = UpdateJournal::new(
            id.clone(),
            JournalOperation::Apply,
            self.target,
            JournalState::Staging,
            before.clone(),
        );
        let mut op = Journalled::new(journal, self.journal_file(), hook);
        let mut switched = false;
        let result = self.run_apply(&mut op, source, options, before, &mut switched);
        match result {
            Ok(report) => {
                lock.release();
                Ok(report)
            }
            Err(UpdateError::Interrupted(checkpoint)) => {
                lock.abandon();
                Err(UpdateError::Interrupted(checkpoint))
            }
            Err(error) => {
                let stage = op.journal.state.as_str();
                self.abort_apply(&mut op, &error, switched);
                self.emit_failed(Some(&id), stage, &error);
                lock.release();
                Err(error)
            }
        }
    }

    fn stage_and_verify(
        &self,
        source: &UpdateSource,
        options: &ApplyOptions,
        staging: &Path,
    ) -> Result<Staged, UpdateError> {
        let resolved = source::resolve(source, self.target, &self.config.limits)?;
        fs::create_dir(staging).map_err(|error| {
            UpdateError::io(format!("cannot create `{}`", staging.display()), error)
        })?;
        let dir = match &resolved.install {
            aidex_bundle::InstallSource::Archive(archive) => {
                aidex_bundle::archive::extract_archive(
                    archive,
                    staging,
                    &self.config.limits.extract,
                )
            }
            directory => steps::stage(directory, staging),
        }
        .map_err(|error| UpdateError::from_package(error, &[]))?;
        let policy = self.policy(options.allow_unsigned)?;
        let candidate = self.verify_dir(&dir, &policy, None, true)?;
        Ok(Staged {
            resolved,
            dir,
            candidate,
        })
    }

    fn decide(
        &self,
        current: Option<&TargetPointer>,
        candidate: &Candidate,
        options: &ApplyOptions,
    ) -> Result<UpdateAction, UpdateError> {
        let manifest = candidate.manifest();
        decide(
            current,
            &manifest.version,
            &candidate.bundle.manifest_sha256,
            options,
            &mut || {
                current.is_some_and(|entry| {
                    let allow = candidate.signature == SignatureState::UnsignedAllowed;
                    self.policy(allow).is_ok_and(|policy| {
                        self.version_path(&entry.dir).is_ok_and(|dir| {
                            self.verify_dir(&dir, &policy, Some(&entry.manifest_sha256), false)
                                .is_ok()
                        })
                    })
                })
            },
        )
    }

    fn run_apply(
        &self,
        op: &mut Journalled<'_>,
        source: &UpdateSource,
        options: &ApplyOptions,
        before: Option<CurrentPointer>,
        switched: &mut bool,
    ) -> Result<ApplyReport, UpdateError> {
        let id = op.journal.id.clone();
        op.enter(JournalState::Staging, Checkpoint::JournalStaging)?;
        let staging = self.staging_root()?.join(id.as_str());
        let staged = self.stage_and_verify(source, options, &staging);
        let staged = staged?;
        let manifest = staged.candidate.manifest().clone();
        self.emit(EventKind::UpdateStaged {
            update_id: id.clone(),
            version: manifest.version.clone(),
            target: manifest.target.to_string(),
        });
        op.checkpoint(Checkpoint::Staged)?;

        let current = before.as_ref().and_then(|p| p.get(self.target)).cloned();
        let from = current.as_ref().map(TargetPointer::current);
        let action = self.decide(current.as_ref(), &staged.candidate, options)?;
        let candidate = staged.candidate.info();
        if action == UpdateAction::UpToDate {
            let _ = remove_any(&staging);
            op.finish(
                JournalState::Aborted,
                format!("{} is already active and intact", manifest.version),
            )?;
            let to = from.clone().ok_or(UpdateError::NotInstalled {
                target: self.target,
            })?;
            return Ok(ApplyReport {
                update_id: id,
                action,
                checksum: staged.resolved.checksum,
                candidate,
                from,
                to,
                self_test: None,
                launchers: Vec::new(),
                pruned: Vec::new(),
                warnings: Vec::new(),
                restart_required: false,
            });
        }
        steps::preflight_launchers(&self.paths, &staged.dir, &manifest)?;
        op.journal.candidate_version = Some(manifest.version.clone());
        op.journal.signer_key_id = candidate.signature.key_id().map(str::to_owned);
        op.enter(JournalState::Verified, Checkpoint::JournalVerified)?;
        let unsigned = candidate.signature == SignatureState::UnsignedAllowed;
        self.emit(EventKind::UpdateVerified {
            update_id: id.clone(),
            version: manifest.version.clone(),
            signer_key_id: op.journal.signer_key_id.clone(),
            unsigned_allowed: unsigned,
        });

        op.enter(JournalState::SelfTest, Checkpoint::JournalSelfTest)?;
        let self_test = self
            .config
            .self_test
            .run(&staged.dir, &manifest, self.paths.root())?;
        op.checkpoint(Checkpoint::SelfTested)?;

        // Switching: decide the version directory, record it, commit it, switch.
        let versions = self.paths.ensure_dir(&layout::versions_dir(&self.paths))?;
        let plan = steps::plan_commit(&versions, &staged.candidate.bundle)?;
        let to = VersionRef {
            version: manifest.version.clone(),
            dir: pointer_dir(plan.name()),
            manifest_sha256: staged.candidate.bundle.manifest_sha256.clone(),
        };
        op.journal.to = Some(to.clone());
        op.journal.created_version_dir = !plan.is_reuse();
        op.enter(JournalState::Switching, Checkpoint::JournalSwitching)?;
        steps::execute_commit(&staged.dir, &versions, &plan)?;
        op.checkpoint(Checkpoint::VersionCommitted)?;

        let mut pointer = self.load_pointer()?.unwrap_or_default();
        match (action, current) {
            // A repair keeps the recorded rollback candidate (the damaged copy is not one).
            (UpdateAction::Repair, Some(entry)) => {
                pointer.targets.insert(
                    self.target.as_str().to_owned(),
                    TargetPointer {
                        version: to.version.clone(),
                        dir: to.dir.clone(),
                        manifest_sha256: to.manifest_sha256.clone(),
                        previous: entry.previous,
                    },
                );
            }
            _ => pointer.switch(self.target, to.clone()),
        }
        op.write_pointer(&pointer, &self.pointer_file())?;
        *switched = true;
        self.emit(EventKind::UpdateSwitched {
            update_id: id.clone(),
            operation: JournalOperation::Apply.as_str().to_owned(),
            from_version: from.as_ref().map(|from| from.version.clone()),
            to_version: to.version.clone(),
        });

        // Committed: everything below is best effort and idempotent.
        let mut warnings = Vec::new();
        if unsigned {
            warnings.push(
                "the package is UNSIGNED and was accepted only because allow_unsigned was set; the launcher refuses it unless AIDEX_ALLOW_UNSIGNED=1".into(),
            );
        }
        let installed = versions.join(plan.name());
        let launchers = self.place_launchers(&installed, &manifest, &mut warnings);
        op.checkpoint(Checkpoint::LaunchersPlaced)?;
        self.copy_provenance(&installed, plan.name(), &mut warnings);
        if let Err(error) = op.finish(
            JournalState::Committed,
            format!("{} is active ({})", to.version, to.dir),
        ) {
            warnings.push(format!(
                "the update journal could not be finalized ({error}); recovery completes it on the next daemon start"
            ));
        }
        op.checkpoint(Checkpoint::JournalCommitted)?;
        let _ = remove_any(&staging);
        let pruned = self.prune(&pointer, id.as_str(), &mut warnings);
        op.checkpoint(Checkpoint::Pruned)?;
        Ok(ApplyReport {
            update_id: id,
            action,
            checksum: staged.resolved.checksum,
            candidate,
            from,
            to,
            self_test: Some(self_test),
            launchers,
            pruned,
            warnings,
            restart_required: true,
        })
    }

    /// Undo a failed apply that did not reach the pointer switch: remove the version
    /// directory it created and its staging directory, and close the journal.
    fn abort_apply(&self, op: &mut Journalled<'_>, error: &UpdateError, switched: bool) {
        if switched {
            // Cannot happen (post-switch steps only warn), but never undo a switch here.
            let _ = op.finish(
                JournalState::Committed,
                format!("committed with error: {error}"),
            );
            return;
        }
        let mut warnings = Vec::new();
        if op.journal.state == JournalState::Switching
            && op.journal.created_version_dir
            && let Some(name) = op
                .journal
                .to
                .as_ref()
                .and_then(|to| to.dir_name().ok().map(str::to_owned))
        {
            let referenced = self.load_pointer().ok().flatten().is_some_and(|pointer| {
                pointer.targets.values().any(|entry| {
                    entry.dir == pointer_dir(&name)
                        || entry
                            .previous
                            .as_ref()
                            .is_some_and(|p| p.dir == pointer_dir(&name))
                })
            });
            if !referenced
                && let Err(error) = self.remove_version_dir(&name, op.journal.id.as_str())
            {
                warnings.push(error.to_string());
            }
        }
        if let Ok(root) = self.staging_root() {
            let _ = remove_any(&root.join(op.journal.id.as_str()));
        }
        let mut outcome = format!("refused: {error}");
        for warning in warnings {
            outcome.push_str("; ");
            outcome.push_str(&warning);
        }
        let _ = op.finish(JournalState::Aborted, outcome);
    }
}
