//! Signed, staged, self-tested AiDEX updates with atomic switching, crash recovery,
//! and rollback, built on `aidex-bundle` (which owns the on-drive formats:
//! `bundle.json`, `bundle.sig`, `app/current.json`, `app/update-journal.json`).
//!
//! An update moves through the journalled states of spec §43:
//!
//! ```text
//! Idle → Staging (extract to app/staging/<id>/) → Verified (signature, trust,
//! revocation, every file hash, target, version rules, drive schema) → SelfTest
//! (staged `aidex --version`, `aidexd --version`, `aidex doctor --json`) →
//! Switching (rename into app/versions/, replace app/current.json atomically; the
//! old bundle becomes `previous`) → Committed (launchers, provenance, pruning)
//! ```
//!
//! [`Updater::recover`] (daemon start) returns any interrupted operation to the old
//! or the new valid version; [`Updater::rollback`] swaps `current` and `previous`.
//! Running processes never swap their own binaries: a switch takes effect on the
//! next launch.

mod apply;
mod checkpoint;
mod error;
mod finalize;
mod journalled;
mod lock;
mod recover;
mod rollback;
pub mod sbom;
mod selftest;
mod service;
mod source;
mod status;
mod version;

pub use apply::{ApplyReport, CheckReport};
pub use checkpoint::{Checkpoint, Hook, Interrupted};
pub use error::UpdateError;
pub use finalize::KEEP_VERSIONS;
pub use lock::LOCK_FILE_NAME;
pub use recover::{RecoveryOutcome, RecoveryReport};
pub use rollback::{RollbackOptions, RollbackReport};
pub use selftest::{ProcessSelfTest, SelfTest, SelfTestCommand, SelfTestFn, SelfTestReport};
pub use service::{CandidateInfo, UpdateLimits, Updater, UpdaterConfig};
pub use source::{ChecksumStatus, UpdateSource};
pub use status::{
    IntegrityReport, JournalSummary, SbomStatus, UpdateStatus, integrity_report, sbom_status,
};
pub use version::{ApplyOptions, UpdateAction};
