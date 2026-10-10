//! `rollback` swaps current and previous after re-verifying the previous bundle.

mod common;

use std::fs;

use aidex_bundle::{JournalOperation, JournalState, UpdateJournal, journal_path, layout};
use aidex_updater::{RollbackOptions, UpdateError};
use common::{Fixture, host};

#[test]
fn rollback_round_trips_between_current_and_previous() {
    let fixture = Fixture::new();
    fixture.install("0.1.0");
    fixture.install("0.2.0");
    let versions = fixture.version_dirs();
    let updater = fixture.updater();

    let report = updater.rollback(&RollbackOptions::default()).unwrap();
    assert_eq!(
        (report.from.version.as_str(), report.to.version.as_str()),
        ("0.2.0", "0.1.0")
    );
    assert!(report.signature.is_signed());
    assert!(report.restart_required);
    assert_eq!(fixture.active_version().as_deref(), Some("0.1.0"));
    let entry = fixture.pointer().unwrap().get(host()).cloned().unwrap();
    assert_eq!(entry.previous.as_ref().unwrap().version, "0.2.0");
    let journal = UpdateJournal::load(&journal_path(&fixture.paths()))
        .unwrap()
        .unwrap();
    assert_eq!(
        (journal.operation, journal.state),
        (JournalOperation::Rollback, JournalState::Committed)
    );

    let report = updater.rollback(&RollbackOptions::default()).unwrap();
    assert_eq!(report.to.version, "0.2.0");
    assert_eq!(fixture.active_version().as_deref(), Some("0.2.0"));
    // Rollback never deletes either version.
    assert_eq!(fixture.version_dirs(), versions);
    assert!(
        fixture
            .event_names()
            .ends_with(&["update.rolled_back", "update.rolled_back"])
    );
}

#[test]
fn rollback_refuses_without_a_previous_or_with_a_damaged_previous() {
    let fixture = Fixture::new();
    assert!(matches!(
        fixture.updater().rollback(&RollbackOptions::default()),
        Err(UpdateError::NotInstalled { .. })
    ));
    fixture.install("0.1.0");
    assert!(matches!(
        fixture.updater().rollback(&RollbackOptions::default()),
        Err(UpdateError::NoPrevious { .. })
    ));
    fixture.install("0.2.0");
    let target = host();
    let previous = fixture
        .root()
        .join("app/versions")
        .join(format!("0.1.0-{target}"));
    let before = fixture.pointer_bytes();
    fs::write(previous.join(layout::bin_path("aidexd", target)), "damaged").unwrap();
    let error = fixture
        .updater()
        .rollback(&RollbackOptions::default())
        .unwrap_err();
    assert!(matches!(error, UpdateError::Tampered { .. }), "{error}");
    assert_eq!(fixture.pointer_bytes(), before);
    assert_eq!(fixture.active_version().as_deref(), Some("0.2.0"));

    // A previous whose bundle.json no longer matches its pin is refused before parsing.
    fs::write(previous.join("bundle.json"), b"{}").unwrap();
    let error = fixture
        .updater()
        .rollback(&RollbackOptions::default())
        .unwrap_err();
    assert!(matches!(error, UpdateError::Tampered { .. }), "{error}");
    assert_eq!(fixture.pointer_bytes(), before);
}
