//! Crash points of an update, for progress reporting and fault-injection tests.
//!
//! [`crate::Updater::apply_with`] and [`crate::Updater::rollback_with`] call a hook at
//! every checkpoint. A hook that returns [`Interrupted`] makes the operation stop
//! right there *without any cleanup* — the lock file, journal, staging directory,
//! partially placed files, and any temporary file stay exactly as a power loss would
//! leave them — and [`crate::Updater::recover`] must then restore a valid version.

use serde::Serialize;

/// A point at which a crash can interrupt an update or rollback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Checkpoint {
    /// The journal says `staging`; nothing is extracted yet.
    JournalStaging,
    /// The package is extracted (or copied) into `app/staging/<id>/`.
    Staged,
    /// The staged bundle verified; the journal says `verified`.
    JournalVerified,
    /// The journal says `self_test`; the self-test has not run yet.
    JournalSelfTest,
    /// The staged binaries passed the self-test.
    SelfTested,
    /// The journal says `switching` and names the version directory.
    JournalSwitching,
    /// The staged bundle was renamed into `app/versions/` (apply only).
    VersionCommitted,
    /// The new `app/current.json` is in its temporary file but not yet renamed over
    /// the old one (a torn pointer write).
    PointerWritten,
    /// `app/current.json` was replaced.
    PointerSwitched,
    /// The drive-root launchers were updated.
    LaunchersPlaced,
    /// The journal says `committed`.
    JournalCommitted,
    /// Old version directories were pruned (apply only).
    Pruned,
}

impl Checkpoint {
    /// Every checkpoint of an apply, in order.
    pub const APPLY: [Checkpoint; 12] = [
        Checkpoint::JournalStaging,
        Checkpoint::Staged,
        Checkpoint::JournalVerified,
        Checkpoint::JournalSelfTest,
        Checkpoint::SelfTested,
        Checkpoint::JournalSwitching,
        Checkpoint::VersionCommitted,
        Checkpoint::PointerWritten,
        Checkpoint::PointerSwitched,
        Checkpoint::LaunchersPlaced,
        Checkpoint::JournalCommitted,
        Checkpoint::Pruned,
    ];

    /// Every checkpoint of a rollback, in order.
    pub const ROLLBACK: [Checkpoint; 6] = [
        Checkpoint::JournalVerified,
        Checkpoint::JournalSwitching,
        Checkpoint::PointerWritten,
        Checkpoint::PointerSwitched,
        Checkpoint::LaunchersPlaced,
        Checkpoint::JournalCommitted,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::JournalStaging => "journal_staging",
            Self::Staged => "staged",
            Self::JournalVerified => "journal_verified",
            Self::JournalSelfTest => "journal_self_test",
            Self::SelfTested => "self_tested",
            Self::JournalSwitching => "journal_switching",
            Self::VersionCommitted => "version_committed",
            Self::PointerWritten => "pointer_written",
            Self::PointerSwitched => "pointer_switched",
            Self::LaunchersPlaced => "launchers_placed",
            Self::JournalCommitted => "journal_committed",
            Self::Pruned => "pruned",
        }
    }

    /// Whether `app/current.json` already selects the new bundle at this point.
    pub fn after_switch(self) -> bool {
        matches!(
            self,
            Self::PointerSwitched | Self::LaunchersPlaced | Self::JournalCommitted | Self::Pruned
        )
    }
}

/// Returned by a checkpoint hook to simulate power loss at that checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Interrupted;

/// A checkpoint hook.
pub type Hook<'a> = &'a mut dyn FnMut(Checkpoint) -> Result<(), Interrupted>;
