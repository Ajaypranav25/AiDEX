//! Power loss at every step of an update and of a rollback, with torn writes: recovery
//! always ends with either the old or the new version active and fully verified, no
//! half-written `current.json`, no staging, temporary files, or lock left behind, and
//! the drive ready for the next update.

mod common;

use std::fs;

use aidex_bundle::{JournalState, UpdateJournal, journal_path, layout};
use aidex_updater::{
    ApplyOptions, Checkpoint, Interrupted, RecoveryOutcome, RollbackOptions, UpdateError,
    UpdateSource,
};
use common::{Fixture, Spec, host};

/// Simulated debris of writes torn by the power loss: partial temporary files of the
/// pointer and the journal (the names `write_atomic` uses), and a half-extracted file
/// in the staging directory.
fn tear(fixture: &Fixture, checkpoint: Checkpoint) {
    let app = fixture.root().join("app");
    fs::write(
        app.join(".current.json.tmp-999-1"),
        b"{\"schema_version\":1,\"tar",
    )
    .unwrap();
    fs::write(app.join(".update-journal.json.tmp-999-2"), b"{\"schema_").unwrap();
    if checkpoint == Checkpoint::PointerWritten {
        // Truncate the real pending pointer write as well.
        for entry in fs::read_dir(&app).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(".current.json.tmp-") && !name.ends_with("-999-1") {
                fs::write(entry.path(), b"{\"schema_version\":1,").unwrap();
            }
        }
    }
    if let Some(staging) = fs::read_dir(app.join("staging"))
        .ok()
        .and_then(|mut entries| entries.next())
        .and_then(Result::ok)
    {
        if staging.file_type().unwrap().is_dir() {
            fs::write(staging.path().join("half-written.part"), b"partial").unwrap();
        }
    }
}

fn assert_recovered(fixture: &Fixture, expected: &str, context: &str) {
    assert_eq!(
        fixture.active_version().as_deref(),
        Some(expected),
        "{context}: active version"
    );
    assert!(
        fixture.staging_entries().is_empty(),
        "{context}: staging {:?}",
        fixture.staging_entries()
    );
    assert_eq!(
        fixture.app_files(),
        ["current.json", "update-journal.json"],
        "{context}: app files"
    );
    let journal = UpdateJournal::load(&journal_path(&fixture.paths()))
        .unwrap()
        .unwrap();
    assert!(
        journal.state.is_terminal(),
        "{context}: journal {:?}",
        journal.state
    );
    // Every version directory left is referenced by the pointer.
    let pointer = fixture.pointer().unwrap();
    let entry = pointer.get(host()).unwrap();
    for name in fixture.version_dirs() {
        let dir = format!("versions/{name}");
        assert!(
            entry.dir == dir || entry.previous.as_ref().is_some_and(|p| p.dir == dir),
            "{context}: stray {dir}"
        );
    }
}

#[test]
fn power_loss_at_every_apply_step_recovers_the_old_or_the_new_version() {
    for checkpoint in Checkpoint::APPLY {
        let context = checkpoint.as_str();
        let fixture = Fixture::new();
        fixture.install("0.1.0");
        let before = fixture.pointer_bytes();
        let v2 = fixture.package("v2", &Spec::new("0.2.0"));
        let error = fixture
            .updater()
            .apply_with(
                &UpdateSource::LocalFile(v2.clone()),
                &ApplyOptions::default(),
                &mut |cp| {
                    if cp == checkpoint {
                        tear(&fixture, cp);
                        Err(Interrupted)
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
        assert!(
            matches!(error, UpdateError::Interrupted(cp) if cp == checkpoint),
            "{context}: {error}"
        );
        // Before recovery, the launcher already sees a whole old or new pointer.
        let seen = fixture
            .active_version()
            .expect("a valid version before recovery");
        assert!(seen == "0.1.0" || seen == "0.2.0", "{context}: {seen}");

        // "Restart": a new updater, as the daemon would create on start.
        let report = fixture.updater().recover().unwrap();
        let expected = if checkpoint.after_switch() {
            "0.2.0"
        } else {
            "0.1.0"
        };
        assert_recovered(&fixture, expected, context);
        let outcome = match checkpoint {
            Checkpoint::JournalCommitted | Checkpoint::Pruned => RecoveryOutcome::Clean,
            Checkpoint::PointerSwitched | Checkpoint::LaunchersPlaced => {
                RecoveryOutcome::RolledForward
            }
            _ => RecoveryOutcome::RolledBack,
        };
        assert_eq!(report.outcome, outcome, "{context}: {report:?}");
        if outcome == RecoveryOutcome::RolledBack {
            assert_eq!(
                fixture.pointer_bytes(),
                before,
                "{context}: pointer restored"
            );
            assert_eq!(report.active.unwrap().version, "0.1.0");
        }
        // The drive accepts the update afterwards.
        let report = fixture
            .updater()
            .apply(&UpdateSource::LocalFile(v2), &ApplyOptions::default())
            .unwrap();
        assert_eq!(
            fixture.active_version().as_deref(),
            Some("0.2.0"),
            "{context}"
        );
        assert_eq!(report.to.version, "0.2.0");
        let names = fixture.event_names();
        if outcome == RecoveryOutcome::RolledBack {
            assert!(
                names.contains(&"update.rolled_back"),
                "{context}: {names:?}"
            );
        }
    }
}

#[test]
fn power_loss_during_a_first_install_leaves_nothing_installed_or_the_new_version() {
    for checkpoint in [
        Checkpoint::Staged,
        Checkpoint::JournalSwitching,
        Checkpoint::VersionCommitted,
        Checkpoint::PointerWritten,
        Checkpoint::PointerSwitched,
    ] {
        let fixture = Fixture::new();
        let v1 = fixture.package("v1", &Spec::new("0.1.0"));
        let _ = fixture.updater().apply_with(
            &UpdateSource::LocalFile(v1),
            &ApplyOptions::default(),
            &mut |cp| {
                if cp == checkpoint {
                    Err(Interrupted)
                } else {
                    Ok(())
                }
            },
        );
        fixture.updater().recover().unwrap();
        if checkpoint.after_switch() {
            assert_eq!(fixture.active_version().as_deref(), Some("0.1.0"));
        } else {
            assert_eq!(fixture.active_version(), None, "{}", checkpoint.as_str());
            assert!(
                fixture
                    .pointer()
                    .is_none_or(|pointer| pointer.get(host()).is_none())
            );
            assert!(fixture.version_dirs().is_empty(), "{}", checkpoint.as_str());
        }
        assert!(fixture.staging_entries().is_empty());
    }
}

#[test]
fn power_loss_at_every_rollback_step_recovers_one_of_the_two_versions() {
    for checkpoint in Checkpoint::ROLLBACK {
        let context = checkpoint.as_str();
        let fixture = Fixture::new();
        fixture.install("0.1.0");
        fixture.install("0.2.0");
        let error = fixture
            .updater()
            .rollback_with(&RollbackOptions::default(), &mut |cp| {
                if cp == checkpoint {
                    tear(&fixture, cp);
                    Err(Interrupted)
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert!(matches!(error, UpdateError::Interrupted(_)), "{context}");
        fixture.updater().recover().unwrap();
        let expected = if checkpoint.after_switch() {
            "0.1.0"
        } else {
            "0.2.0"
        };
        assert_recovered(&fixture, expected, context);
        let entry = fixture.pointer().unwrap().get(host()).cloned().unwrap();
        let other = if expected == "0.1.0" {
            "0.2.0"
        } else {
            "0.1.0"
        };
        assert_eq!(entry.previous.unwrap().version, other, "{context}");
        // Rolling back again works.
        fixture
            .updater()
            .rollback(&RollbackOptions::default())
            .unwrap();
        assert_eq!(
            fixture.active_version().as_deref(),
            Some(other),
            "{context}"
        );
    }
}

#[test]
fn a_pointer_torn_by_the_filesystem_is_restored_from_the_journal() {
    let fixture = Fixture::new();
    fixture.install("0.1.0");
    let before = fixture.pointer_bytes().unwrap();
    let v2 = fixture.package("v2", &Spec::new("0.2.0"));
    let pointer_file = layout::pointer_path(&fixture.paths());
    let _ = fixture.updater().apply_with(
        &UpdateSource::LocalFile(v2),
        &ApplyOptions::default(),
        &mut |cp| {
            if cp == Checkpoint::PointerWritten {
                // A filesystem that does not rename atomically left garbage behind.
                fs::write(&pointer_file, b"{\"schema_version\":1,\"targ").unwrap();
                Err(Interrupted)
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(
        fixture.active_version(),
        None,
        "the launcher refuses the torn pointer"
    );
    let report = fixture.updater().recover().unwrap();
    assert_eq!(report.outcome, RecoveryOutcome::RolledBack);
    assert!(report.restored_pointer);
    assert_eq!(fixture.pointer_bytes().unwrap(), before);
    assert_recovered(&fixture, "0.1.0", "torn pointer");
}

#[test]
fn an_unreadable_journal_is_set_aside_without_touching_the_pointer() {
    let fixture = Fixture::new();
    fixture.install("0.1.0");
    let before = fixture.pointer_bytes();
    let journal = journal_path(&fixture.paths());
    fs::write(&journal, b"{\"schema_version\":1,\"id\":").unwrap();
    let report = fixture.updater().recover().unwrap();
    assert_eq!(report.outcome, RecoveryOutcome::JournalSetAside);
    assert!(!journal.exists());
    assert_eq!(fixture.pointer_bytes(), before);
    assert_eq!(fixture.active_version().as_deref(), Some("0.1.0"));
    let aside = fixture
        .app_files()
        .into_iter()
        .any(|name| name.starts_with("update-journal.json.corrupt-"));
    assert!(aside);
    // A clean drive recovers to `clean`, idempotently.
    assert_eq!(
        fixture.updater().recover().unwrap().outcome,
        RecoveryOutcome::Clean
    );
}

#[test]
fn a_switch_whose_new_version_was_damaged_meanwhile_rolls_back() {
    let fixture = Fixture::new();
    fixture.install("0.1.0");
    let v2 = fixture.package("v2", &Spec::new("0.2.0"));
    let _ = fixture.updater().apply_with(
        &UpdateSource::LocalFile(v2),
        &ApplyOptions::default(),
        &mut |cp| {
            if cp == Checkpoint::PointerSwitched {
                Err(Interrupted)
            } else {
                Ok(())
            }
        },
    );
    let target = host();
    fs::write(
        fixture
            .root()
            .join("app/versions")
            .join(format!("0.2.0-{target}"))
            .join(layout::bin_path("aidex", target)),
        "damaged after the switch",
    )
    .unwrap();
    let report = fixture.updater().recover().unwrap();
    assert_eq!(report.outcome, RecoveryOutcome::RolledBack);
    assert_eq!(report.interrupted_state, Some(JournalState::Switching));
    assert_recovered(&fixture, "0.1.0", "damaged new version");
}
