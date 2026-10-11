//! The real process self-test runs the staged `aidex` and `aidexd` (a test stand-in)
//! with `AIDEX_DRIVE_ROOT` and refuses the update when they fail or hang.

mod common;

use std::{sync::Arc, time::Duration};

use aidex_updater::{ApplyOptions, ProcessSelfTest, UpdateError, UpdateSource, UpdaterConfig};
use common::{Fixture, Spec};

fn process_updater(fixture: &Fixture, timeout: Duration) -> aidex_updater::Updater {
    let config =
        UpdaterConfig::new(fixture.trust()).with_self_test(Arc::new(ProcessSelfTest { timeout }));
    fixture.updater_with(config)
}

#[test]
fn the_staged_binaries_pass_the_process_self_test() {
    let fixture = Fixture::new();
    let package = fixture.package("good", &Spec::new("0.3.0").real_binaries());
    let report = process_updater(&fixture, Duration::from_secs(60))
        .apply(&UpdateSource::LocalFile(package), &ApplyOptions::default())
        .unwrap();
    let commands: Vec<_> = report
        .self_test
        .unwrap()
        .commands
        .into_iter()
        .map(|command| (command.command, command.exit_code))
        .collect();
    assert_eq!(
        commands,
        [
            ("aidex --version".to_owned(), Some(0)),
            ("aidexd --version".to_owned(), Some(0)),
            ("aidex doctor --json".to_owned(), Some(1)),
        ]
    );
    assert_eq!(fixture.active_version().as_deref(), Some("0.3.0"));
}

#[test]
fn a_failing_or_hanging_staged_binary_stops_the_update() {
    let fixture = Fixture::new();
    fixture.install("0.1.0");
    let before = fixture.pointer_bytes();
    let failing = fixture.package("fail", &Spec::new("0.2.0-selftest-fail").real_binaries());
    let error = process_updater(&fixture, Duration::from_secs(60))
        .apply(&UpdateSource::LocalFile(failing), &ApplyOptions::default())
        .unwrap_err();
    assert!(
        matches!(&error, UpdateError::SelfTestFailed { command, .. } if command == "aidex --version"),
        "{error}"
    );
    assert_eq!(fixture.pointer_bytes(), before);

    let hanging = fixture.package("hang", &Spec::new("0.2.0-selftest-hang").real_binaries());
    let started = std::time::Instant::now();
    let error = process_updater(&fixture, Duration::from_secs(2))
        .apply(&UpdateSource::LocalFile(hanging), &ApplyOptions::default())
        .unwrap_err();
    assert!(
        matches!(&error, UpdateError::SelfTestFailed { detail, .. } if detail.contains("did not finish")),
        "{error}"
    );
    assert!(started.elapsed() < Duration::from_secs(60));
    assert_eq!(fixture.pointer_bytes(), before);
    assert!(fixture.staging_entries().is_empty());
    assert_eq!(fixture.active_version().as_deref(), Some("0.1.0"));
}
