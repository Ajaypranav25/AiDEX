//! `rollback`: make the recorded previous bundle active again (current ↔ previous).

use aidex_bundle::{
    JournalOperation, JournalState, LauncherPlacement, SignatureState, UPDATE_ID_PREFIX,
    UpdateJournal, VersionRef,
};
use aidex_core::EntityId;
use aidex_events::EventKind;
use serde::{Deserialize, Serialize};

use crate::{Checkpoint, Hook, UpdateError, Updater, journalled::Journalled, lock::DriveLock};

/// Options for [`Updater::rollback`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackOptions {
    /// Development only: accept a previous bundle without `bundle.sig`.
    #[serde(default)]
    pub allow_unsigned: bool,
}

/// What [`Updater::rollback`] did.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RollbackReport {
    pub update_id: EntityId,
    /// The bundle that was active.
    pub from: VersionRef,
    /// The bundle that is active now (the former previous); `from` became previous.
    pub to: VersionRef,
    pub signature: SignatureState,
    pub launchers: Vec<LauncherPlacement>,
    pub warnings: Vec<String>,
    pub restart_required: bool,
}

impl Updater {
    /// Swap `current` and `previous` after verifying that the previous bundle still
    /// matches its pinned manifest, signature, and every file hash.
    pub fn rollback(&self, options: &RollbackOptions) -> Result<RollbackReport, UpdateError> {
        self.rollback_with(options, &mut |_| Ok(()))
    }

    /// [`rollback`](Self::rollback) with a checkpoint hook.
    pub fn rollback_with(
        &self,
        options: &RollbackOptions,
        hook: Hook<'_>,
    ) -> Result<RollbackReport, UpdateError> {
        let _gate = self.gate()?;
        self.ensure_no_pending()?;
        let app = self.app()?;
        let id = EntityId::generate(UPDATE_ID_PREFIX).map_err(|error| {
            UpdateError::io(
                "cannot name the rollback",
                std::io::Error::other(error.to_string()),
            )
        })?;
        let lock = DriveLock::acquire(&app, &id)?;
        let result = self.run_rollback(&id, options, hook);
        match &result {
            Err(UpdateError::Interrupted(_)) => lock.abandon(),
            Err(error) => {
                self.emit_failed(Some(&id), "rollback", error);
                lock.release();
            }
            Ok(_) => lock.release(),
        }
        result
    }

    fn run_rollback(
        &self,
        id: &EntityId,
        options: &RollbackOptions,
        hook: Hook<'_>,
    ) -> Result<RollbackReport, UpdateError> {
        let not_installed = || UpdateError::NotInstalled {
            target: self.target,
        };
        let before = self.load_pointer()?.ok_or_else(not_installed)?;
        let entry = before.get(self.target).cloned().ok_or_else(not_installed)?;
        let previous = entry.previous.clone().ok_or(UpdateError::NoPrevious {
            target: self.target,
        })?;
        let previous_dir = self.version_path(&previous.dir)?;
        let policy = self.policy(options.allow_unsigned)?;
        let candidate = self.verify_dir(
            &previous_dir,
            &policy,
            Some(&previous.manifest_sha256),
            false,
        )?;
        if candidate.manifest().version != previous.version {
            return Err(UpdateError::Bundle(
                aidex_bundle::BundleError::VersionMismatch {
                    pointer: previous.version.clone(),
                    manifest: candidate.manifest().version.clone(),
                },
            ));
        }
        aidex_bundle::steps::preflight_launchers(&self.paths, &previous_dir, candidate.manifest())?;

        let mut journal = UpdateJournal::new(
            id.clone(),
            JournalOperation::Rollback,
            self.target,
            JournalState::Verified,
            Some(before.clone()),
        );
        journal.candidate_version = Some(previous.version.clone());
        journal.signer_key_id = candidate.signature.key_id().map(str::to_owned);
        journal.to = Some(previous.clone());
        let mut op = Journalled::new(journal, self.journal_file(), hook);
        let mut pointer = before;
        pointer.switch(self.target, previous.clone());
        let switched = (|| {
            op.enter(JournalState::Verified, Checkpoint::JournalVerified)?;
            op.enter(JournalState::Switching, Checkpoint::JournalSwitching)?;
            op.write_pointer(&pointer, &self.pointer_file())
        })();
        if let Err(error) = switched {
            if !matches!(error, UpdateError::Interrupted(_)) {
                // The pointer was not replaced; close the journal.
                let _ = op.finish(JournalState::Aborted, format!("rollback refused: {error}"));
            }
            return Err(error);
        }
        let from = entry.current();
        self.emit(EventKind::UpdateRolledBack {
            update_id: id.clone(),
            from_version: Some(from.version.clone()),
            to_version: Some(previous.version.clone()),
            reason: "requested".into(),
        });

        let mut warnings = Vec::new();
        let launchers = self.place_launchers(&previous_dir, candidate.manifest(), &mut warnings);
        op.checkpoint(Checkpoint::LaunchersPlaced)?;
        if let Err(error) = op.finish(
            JournalState::Committed,
            format!("rolled back from {} to {}", from.version, previous.version),
        ) {
            warnings.push(format!(
                "the update journal could not be finalized ({error}); recovery completes it on the next daemon start"
            ));
        }
        op.checkpoint(Checkpoint::JournalCommitted)?;
        Ok(RollbackReport {
            update_id: id.clone(),
            from,
            to: previous,
            signature: candidate.signature,
            launchers,
            warnings,
            restart_required: true,
        })
    }
}
