//! `app/update-journal.json`: the durable record of an update or rollback in progress.
//!
//! The updater writes the journal (temporary file + fsync + rename) *before* each
//! step, so after a crash it names the step that may be incomplete; recovery then
//! rolls back to the pointer recorded in `before`, or — when the pointer switch had
//! already happened and the new bundle verifies — finishes the operation. A journal
//! in a terminal state (`committed`, `aborted`) is kept as the record of the last
//! operation. The launcher only reads it (to report an interrupted update); it never
//! writes it.
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "id": "upd_01J...",
//!   "operation": "apply",
//!   "target": "windows-x64",
//!   "state": "switching",
//!   "started_at": "2026-10-10T08:00:00.000Z",
//!   "updated_at": "2026-10-10T08:00:03.000Z",
//!   "staging": "staging/upd_01J...",
//!   "before": { "schema_version": 1, "targets": { "...": "the whole previous pointer" } },
//!   "candidate_version": "0.2.0",
//!   "to": { "version": "0.2.0", "dir": "versions/0.2.0-windows-x64", "manifest_sha256": "..." },
//!   "created_version_dir": true,
//!   "signer_key_id": "3f9a0c1b2d4e5f60",
//!   "outcome": null
//! }
//! ```

use std::{
    fs,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
};

use aidex_core::{EntityId, Timestamp};
use aidex_drive::DrivePaths;
use serde::{Deserialize, Serialize};

use crate::{
    BundleError, CurrentPointer, DocumentError, Target, VersionRef, digest::is_lower_hex,
    fsutil::write_atomic, layout::STAGING_DIR, manifest::parse_versioned, path::validate_version,
};

/// File name of the journal inside the drive's `app/` directory.
pub const JOURNAL_FILE_NAME: &str = "update-journal.json";
/// The journal schema this build writes and reads.
pub const JOURNAL_SCHEMA_VERSION: u32 = 1;
/// Prefix of update ids and of the staging directories the updater owns.
pub const UPDATE_ID_PREFIX: &str = "upd";
const MAX_JOURNAL_BYTES: u64 = 2 * 1024 * 1024;
const MAX_OUTCOME_LEN: usize = 2048;

/// What the journalled operation does.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalOperation {
    /// Install a verified update package (or a repair or permitted downgrade).
    Apply,
    /// Swap `current` and `previous`.
    Rollback,
}

impl JournalOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Rollback => "rollback",
        }
    }
}

/// The step an operation is in. Non-terminal states name the step that was started
/// and may be incomplete.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    /// Extracting or copying the package into `app/staging/<id>/`.
    Staging,
    /// The staged bundle passed every check (signature, hashes, target, version).
    Verified,
    /// Running the staged binaries' self-test.
    SelfTest,
    /// Committing the version directory and replacing `app/current.json`.
    Switching,
    /// Finished: the pointer selects the new bundle.
    Committed,
    /// Finished without changing the active bundle (refused, failed, or recovered).
    Aborted,
}

impl JournalState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::Aborted)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Verified => "verified",
            Self::SelfTest => "self_test",
            Self::Switching => "switching",
            Self::Committed => "committed",
            Self::Aborted => "aborted",
        }
    }
}

/// The parsed journal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateJournal {
    pub schema_version: u32,
    /// `upd_<ULID>`; also the staging directory name.
    pub id: EntityId,
    pub operation: JournalOperation,
    pub target: Target,
    pub state: JournalState,
    pub started_at: Timestamp,
    pub updated_at: Timestamp,
    /// `staging/<id>` relative to `app/` (apply only).
    pub staging: Option<String>,
    /// The whole `app/current.json` before the operation (`None`: there was none).
    pub before: Option<CurrentPointer>,
    /// Version of the package being applied, once its manifest was read.
    pub candidate_version: Option<String>,
    /// The bundle the operation switches to, once its directory is decided.
    pub to: Option<VersionRef>,
    /// Whether this operation created `to.dir` (and may remove it on rollback).
    pub created_version_dir: bool,
    /// The key that signed the candidate (`None`: unsigned, allowed by a dev flag).
    pub signer_key_id: Option<String>,
    /// Human-readable result of a terminal state.
    pub outcome: Option<String>,
}

impl UpdateJournal {
    /// A new journal in `state` for operation `id`.
    pub fn new(
        id: EntityId,
        operation: JournalOperation,
        target: Target,
        state: JournalState,
        before: Option<CurrentPointer>,
    ) -> Self {
        let now = Timestamp::now();
        let staging = match operation {
            JournalOperation::Apply => Some(format!("{STAGING_DIR}/{}", id.as_str())),
            JournalOperation::Rollback => None,
        };
        Self {
            schema_version: JOURNAL_SCHEMA_VERSION,
            id,
            operation,
            target,
            state,
            started_at: now.clone(),
            updated_at: now,
            staging,
            before,
            candidate_version: None,
            to: None,
            created_version_dir: false,
            signer_key_id: None,
            outcome: None,
        }
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, DocumentError> {
        let journal: Self = parse_versioned(bytes, JOURNAL_SCHEMA_VERSION)?;
        journal.validate()?;
        Ok(journal)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, DocumentError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| DocumentError::Malformed(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        let invalid = |reason: String| Err(DocumentError::Invalid(reason));
        if self.schema_version != JOURNAL_SCHEMA_VERSION {
            return Err(DocumentError::UnsupportedSchema {
                found: Some(u64::from(self.schema_version)),
                supported: JOURNAL_SCHEMA_VERSION,
            });
        }
        let prefix = format!("{UPDATE_ID_PREFIX}_");
        if !self.id.as_str().starts_with(&prefix) {
            return invalid(format!("journal id `{}` is not an update id", self.id));
        }
        let expected_staging = format!("{STAGING_DIR}/{}", self.id.as_str());
        match (&self.operation, &self.staging) {
            (JournalOperation::Apply, Some(staging)) if *staging == expected_staging => {}
            (JournalOperation::Rollback, None) => {}
            _ => {
                return invalid(format!(
                    "staging must be `{expected_staging}` for apply and absent for rollback"
                ));
            }
        }
        if let Some(before) = &self.before {
            before.validate()?;
        }
        if let Some(version) = &self.candidate_version {
            validate_version(version).map_err(DocumentError::Invalid)?;
        }
        if let Some(to) = &self.to {
            to.validate()?;
        }
        if self
            .signer_key_id
            .as_deref()
            .is_some_and(|id| !is_lower_hex(id, 8))
        {
            return invalid("signer_key_id must be 16 lowercase hex digits".into());
        }
        if self
            .outcome
            .as_deref()
            .is_some_and(|text| text.len() > MAX_OUTCOME_LEN)
        {
            return invalid(format!("outcome is longer than {MAX_OUTCOME_LEN} bytes"));
        }
        Ok(())
    }

    /// Move to `state` and refresh `updated_at` (callers then [`store`](Self::store)).
    pub fn advance(&mut self, state: JournalState) {
        self.state = state;
        self.updated_at = Timestamp::now();
    }

    /// Finish in a terminal `state` with a human-readable outcome (bounded).
    pub fn finish(&mut self, state: JournalState, outcome: impl Into<String>) {
        let mut outcome: String = outcome.into();
        if outcome.len() > MAX_OUTCOME_LEN {
            let mut cut = MAX_OUTCOME_LEN;
            while !outcome.is_char_boundary(cut) {
                cut -= 1;
            }
            outcome.truncate(cut);
        }
        self.outcome = Some(outcome);
        self.advance(state);
    }

    /// Read the journal; `Ok(None)` when there is none. A link, non-file, oversized,
    /// or malformed journal is an error the caller must handle (recovery sets it aside).
    pub fn load(path: &Path) -> Result<Option<Self>, BundleError> {
        let journal_error = |error: DocumentError| BundleError::Journal {
            path: path.to_path_buf(),
            error,
        };
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(BundleError::io(
                    format!("cannot inspect `{}`", path.display()),
                    error,
                ));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(journal_error(DocumentError::Malformed(
                "not a regular file".into(),
            )));
        }
        if metadata.len() > MAX_JOURNAL_BYTES {
            return Err(journal_error(DocumentError::Malformed(format!(
                "larger than {MAX_JOURNAL_BYTES} bytes"
            ))));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .and_then(|file| file.take(MAX_JOURNAL_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|error| BundleError::io(format!("cannot read `{}`", path.display()), error))?;
        Self::from_json(&bytes).map(Some).map_err(journal_error)
    }

    /// Atomically replace the journal at `path`.
    pub fn store(&self, path: &Path) -> Result<(), BundleError> {
        let bytes = self.to_json().map_err(|error| BundleError::Journal {
            path: path.to_path_buf(),
            error,
        })?;
        write_atomic(path, &bytes)
    }
}

/// The drive's update journal, `app/update-journal.json`.
pub fn journal_path(paths: &DrivePaths) -> PathBuf {
    paths.app().join(JOURNAL_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::sha256_hex;

    fn id() -> EntityId {
        EntityId::generate(UPDATE_ID_PREFIX).unwrap()
    }

    #[test]
    fn journals_round_trip_and_validate_their_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(JOURNAL_FILE_NAME);
        assert_eq!(UpdateJournal::load(&path).unwrap(), None);
        let mut journal = UpdateJournal::new(
            id(),
            JournalOperation::Apply,
            Target::LinuxX64,
            JournalState::Staging,
            Some(CurrentPointer::default()),
        );
        journal.candidate_version = Some("0.2.0".into());
        journal.to = Some(VersionRef {
            version: "0.2.0".into(),
            dir: "versions/0.2.0-linux-x64".into(),
            manifest_sha256: sha256_hex(b"m"),
        });
        journal.advance(JournalState::Switching);
        journal.store(&path).unwrap();
        assert_eq!(UpdateJournal::load(&path).unwrap(), Some(journal.clone()));
        assert!(!journal.state.is_terminal());
        journal.finish(JournalState::Committed, "x".repeat(5000));
        assert!(journal.state.is_terminal());
        assert_eq!(journal.outcome.as_ref().unwrap().len(), MAX_OUTCOME_LEN);

        let mut bad = journal.clone();
        bad.staging = Some("staging/../../evil".into());
        assert!(bad.to_json().is_err());
        let mut bad = journal.clone();
        bad.to.as_mut().unwrap().dir = "../outside".into();
        assert!(bad.to_json().is_err());
        let mut bad = journal;
        bad.id = EntityId::generate("stage").unwrap();
        assert!(bad.to_json().is_err());

        fs::write(&path, b"{\"schema_version\":1,").unwrap();
        assert!(matches!(
            UpdateJournal::load(&path),
            Err(BundleError::Journal { .. })
        ));
    }
}
