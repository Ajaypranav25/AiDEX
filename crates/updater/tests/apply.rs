//! Applying signed packages: install, upgrade, previous/pruning, idempotence, repair,
//! sources, provenance, and status.

mod common;

use std::fs;

use aidex_bundle::{JournalState, KeySource, SignatureState, UpdateJournal, journal_path};
use aidex_updater::{
    ApplyOptions, ChecksumStatus, KEEP_VERSIONS, LOCK_FILE_NAME, UpdateAction, UpdateError,
    UpdateSource,
};
use common::{Fixture, Spec, host};

#[test]
fn install_then_upgrade_switches_the_pointer_and_keeps_the_previous_version() {
    let fixture = Fixture::new();
    let updater = fixture.updater();
    let v1 = fixture.package("v1", &Spec::new("0.1.0"));
    let report = updater
        .apply(&UpdateSource::LocalFile(v1), &ApplyOptions::default())
        .unwrap();
    assert_eq!(report.action, UpdateAction::Install);
    assert_eq!(report.checksum, ChecksumStatus::Verified);
    assert_eq!(report.from, None);
    assert!(report.restart_required);
    assert_eq!(
        report.candidate.signature,
        SignatureState::Verified {
            key_id: fixture.key.key_id().into(),
            key_name: "updater-tests".into(),
            key_source: KeySource::Explicit,
        }
    );
    assert_eq!(fixture.active_version().as_deref(), Some("0.1.0"));

    let v2 = fixture.package("v2", &Spec::new("0.2.0"));
    let report = updater
        .apply(&UpdateSource::LocalFile(v2), &ApplyOptions::default())
        .unwrap();
    assert_eq!(report.action, UpdateAction::Upgrade);
    assert_eq!(report.from.as_ref().unwrap().version, "0.1.0");
    assert_eq!(report.to.version, "0.2.0");
    assert_eq!(fixture.active_version().as_deref(), Some("0.2.0"));
    let pointer = fixture.pointer().unwrap();
    let entry = pointer.get(host()).unwrap();
    assert_eq!(entry.previous.as_ref().unwrap().version, "0.1.0");
    assert!(fixture.staging_entries().is_empty());
    assert!(
        !fixture
            .app_files()
            .iter()
            .any(|name| name == LOCK_FILE_NAME)
    );
    let journal = UpdateJournal::load(&journal_path(&fixture.paths()))
        .unwrap()
        .unwrap();
    assert_eq!(journal.state, JournalState::Committed);
    assert_eq!(journal.signer_key_id.as_deref(), Some(fixture.key.key_id()));
    assert_eq!(
        fixture.event_names(),
        [
            "update.staged",
            "update.verified",
            "update.switched",
            "update.staged",
            "update.verified",
            "update.switched",
        ]
    );
}

#[test]
fn checking_a_package_reports_the_action_and_changes_nothing() {
    let fixture = Fixture::new();
    fixture.install("0.1.0");
    let before = fixture.pointer_bytes();
    let dirs = fixture.version_dirs();
    let v2 = fixture.package("v2", &Spec::new("0.2.0"));
    let report = fixture
        .updater()
        .check(&UpdateSource::LocalFile(v2), &ApplyOptions::default())
        .unwrap();
    assert_eq!(report.action, UpdateAction::Upgrade);
    assert_eq!(report.candidate.version, "0.2.0");
    assert!(report.candidate.sbom);
    assert_eq!(report.current.unwrap().version, "0.1.0");
    assert_eq!(fixture.pointer_bytes(), before);
    assert_eq!(fixture.version_dirs(), dirs);
    assert!(fixture.staging_entries().is_empty());
}

#[test]
fn only_current_and_previous_versions_are_kept_after_a_commit() {
    let fixture = Fixture::new();
    for version in ["0.1.0", "0.2.0", "0.3.0", "0.4.0"] {
        fixture.install(version);
    }
    let target = host();
    assert_eq!(
        fixture.version_dirs(),
        [format!("0.3.0-{target}"), format!("0.4.0-{target}")]
    );
    assert_eq!(fixture.version_dirs().len(), KEEP_VERSIONS);
    assert_eq!(fixture.active_version().as_deref(), Some("0.4.0"));
    // Provenance of every installed version stays in manifests/.
    for version in ["0.1.0", "0.2.0", "0.3.0", "0.4.0"] {
        let dir = fixture
            .root()
            .join("manifests")
            .join(format!("{version}-{target}"));
        for file in ["bundle.json", "bundle.sig", "sbom.cdx.json"] {
            assert!(dir.join(file).is_file(), "{version}: {file}");
        }
        let sbom =
            aidex_updater::sbom::Sbom::from_json(&fs::read(dir.join("sbom.cdx.json")).unwrap())
                .unwrap();
        assert_eq!(sbom.validate().unwrap().subject, format!("aidex@{version}"));
    }
}

#[test]
fn the_active_package_again_is_up_to_date_and_a_damaged_install_is_repaired() {
    let fixture = Fixture::new();
    fixture.install("0.1.0");
    fixture.install("0.2.0");
    let v2 = fixture.package("v2-again", &Spec::new("0.2.0"));
    let updater = fixture.updater();
    let report = updater
        .apply(
            &UpdateSource::LocalFile(v2.clone()),
            &ApplyOptions::default(),
        )
        .unwrap();
    assert_eq!(report.action, UpdateAction::UpToDate);
    assert!(!report.restart_required);

    // A different build of the active version is not an update...
    let rebuild = fixture.package("v2-rebuild", &Spec::new("0.2.0").tag("b"));
    assert!(matches!(
        updater.apply(&UpdateSource::LocalFile(rebuild), &ApplyOptions::default()),
        Err(UpdateError::NotNewer { .. })
    ));
    // ...but the same package repairs a damaged installation, keeping `previous`.
    let target = host();
    let daemon = fixture
        .root()
        .join("app/versions")
        .join(format!("0.2.0-{target}"))
        .join(aidex_bundle::layout::bin_path("aidexd", target));
    fs::write(&daemon, "corrupted").unwrap();
    assert_eq!(fixture.active_version(), None);
    let report = updater
        .apply(&UpdateSource::LocalFile(v2), &ApplyOptions::default())
        .unwrap();
    assert_eq!(report.action, UpdateAction::Repair);
    assert_eq!(fixture.active_version().as_deref(), Some("0.2.0"));
    let entry = fixture.pointer().unwrap().get(target).cloned().unwrap();
    assert_ne!(entry.dir, format!("versions/0.2.0-{target}"));
    assert_eq!(entry.previous.unwrap().version, "0.1.0");
    // The damaged copy was pruned.
    assert!(!fixture.version_dirs().contains(&format!("0.2.0-{target}")));
}

#[test]
fn a_release_directory_source_uses_the_newest_package_for_the_target() {
    let fixture = Fixture::new();
    let folder = fixture.out.path().join("release");
    fs::create_dir_all(&folder).unwrap();
    for version in ["0.1.0", "0.10.0", "0.9.0"] {
        let package = fixture.package(&format!("rel-{version}"), &Spec::new(version));
        fs::copy(&package, folder.join(package.file_name().unwrap())).unwrap();
    }
    let other = fixture.package("other", &Spec::new("9.0.0").target(common::other_target()));
    fs::copy(&other, folder.join(other.file_name().unwrap())).unwrap();
    let report = fixture
        .updater()
        .apply(&UpdateSource::Directory(folder), &ApplyOptions::default())
        .unwrap();
    assert_eq!(report.to.version, "0.10.0");
    assert_eq!(report.checksum, ChecksumStatus::Absent);

    // An unpacked bundle directory is a source too.
    let dir = fixture.bundle_dir("unpacked", &Spec::new("0.11.0"), true);
    let report = fixture
        .updater()
        .apply(&UpdateSource::Directory(dir), &ApplyOptions::default())
        .unwrap();
    assert_eq!(report.to.version, "0.11.0");
    assert_eq!(report.checksum, ChecksumStatus::NotApplicable);

    let empty = fixture.out.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert!(matches!(
        fixture
            .updater()
            .apply(&UpdateSource::Directory(empty), &ApplyOptions::default()),
        Err(UpdateError::NoPackage { .. })
    ));
}

#[test]
fn status_reports_versions_signer_sbom_and_the_last_operation() {
    let fixture = Fixture::new();
    fixture.install("0.1.0");
    fixture.install("0.2.0");
    let status = fixture.updater().status().unwrap();
    assert_eq!(status.current.unwrap().version, "0.2.0");
    assert_eq!(status.previous.unwrap().version, "0.1.0");
    assert!(!status.pending);
    assert_eq!(status.journal.unwrap().state, JournalState::Committed);
    let integrity = status.integrity;
    assert!(integrity.verified, "{integrity:?}");
    assert_eq!(
        integrity.signer_key_id.as_deref(),
        Some(fixture.key.key_id())
    );
    assert!(
        integrity.sbom.present && integrity.sbom.valid,
        "{:?}",
        integrity.sbom
    );
    assert_eq!(status.versions.len(), 2);
    // The test process does not run from the drive.
    assert_eq!(status.running_dir, None);
    assert!(!status.restart_required);
}

#[test]
fn an_unfinished_journal_blocks_new_operations_until_recovery() {
    let fixture = Fixture::new();
    fixture.install("0.1.0");
    let v2 = fixture.package("v2", &Spec::new("0.2.0"));
    let updater = fixture.updater();
    let error = updater
        .apply_with(
            &UpdateSource::LocalFile(v2.clone()),
            &ApplyOptions::default(),
            &mut |cp| {
                if cp == aidex_updater::Checkpoint::JournalSelfTest {
                    Err(aidex_updater::Interrupted)
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
    assert!(matches!(error, UpdateError::Interrupted(_)));
    assert!(matches!(
        updater.apply(
            &UpdateSource::LocalFile(v2.clone()),
            &ApplyOptions::default()
        ),
        Err(UpdateError::RecoveryRequired { .. })
    ));
    updater.recover().unwrap();
    updater
        .apply(&UpdateSource::LocalFile(v2), &ApplyOptions::default())
        .unwrap();
    assert_eq!(fixture.active_version().as_deref(), Some("0.2.0"));
}
