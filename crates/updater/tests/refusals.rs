//! Every verification refusal is typed, happens before anything is switched, and
//! leaves the drive exactly as it was (pointer bytes, versions, no staging, no lock).

mod common;

use std::{fs, path::PathBuf};

use aidex_bundle::{
    JournalState, KeyPair, MANIFEST_FILE_NAME, Revocation, RevocationList, SIGNATURE_FILE_NAME,
    SignatureError, UpdateJournal, archive::bundle_root_name, journal_path, layout,
    revocations_path,
};
use aidex_updater::{
    ApplyOptions, SelfTestFn, UpdateError, UpdateLimits, UpdateSource, UpdaterConfig,
};
use common::{Fixture, Spec, host, raw_archive, zip_entries};
use serde_json::Value;

/// A drive with 0.1.0 installed; `refuse` applies `package` and checks that the
/// refusal changed nothing and was recorded.
struct Installed {
    fixture: Fixture,
    /// The 0.1.0 package that was installed.
    v1: PathBuf,
}

impl Installed {
    fn new() -> Self {
        let fixture = Fixture::new();
        let v1 = fixture.install("0.1.0");
        Self { fixture, v1 }
    }

    fn refuse(&self, package: PathBuf, options: ApplyOptions) -> UpdateError {
        let fixture = &self.fixture;
        let pointer = fixture.pointer_bytes();
        let versions = fixture.version_dirs();
        let error = fixture
            .updater()
            .apply(&UpdateSource::LocalFile(package), &options)
            .unwrap_err();
        assert_eq!(fixture.pointer_bytes(), pointer, "{error}");
        assert_eq!(fixture.version_dirs(), versions, "{error}");
        assert!(fixture.staging_entries().is_empty(), "{error}");
        assert_eq!(
            fixture.app_files(),
            ["current.json", "update-journal.json"],
            "{error}"
        );
        assert_eq!(fixture.active_version().as_deref(), Some("0.1.0"));
        let journal = UpdateJournal::load(&journal_path(&fixture.paths()))
            .unwrap()
            .unwrap();
        assert_eq!(journal.state, JournalState::Aborted, "{error}");
        assert_eq!(fixture.event_names().last(), Some(&"update.failed"));
        error
    }

    /// A signed 0.2.0 bundle directory to tamper with before archiving.
    fn source(&self, name: &str) -> (PathBuf, Spec) {
        let spec = Spec::new("0.2.0");
        (self.fixture.bundle_dir(name, &spec, true), spec)
    }

    fn raw(&self, name: &str, dir: &std::path::Path, spec: &Spec) -> PathBuf {
        let folder = self.fixture.out.path().join(name);
        fs::create_dir_all(&folder).unwrap();
        let out = folder.join(aidex_bundle::archive::archive_file_name(
            &spec.version,
            spec.target,
        ));
        raw_archive(dir, &bundle_root_name(&spec.version, spec.target), &out);
        out
    }
}

fn edit_manifest(dir: &std::path::Path, edit: impl FnOnce(&mut Value)) {
    let path = dir.join(MANIFEST_FILE_NAME);
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut manifest);
    fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
}

#[test]
fn a_modified_manifest_fails_the_signature() {
    let installed = Installed::new();
    let (dir, spec) = installed.source("mismatch");
    edit_manifest(&dir, |manifest| {
        manifest["rustc"] = "rustc (attacker)".into()
    });
    let package = installed.fixture.archive("mismatch", &dir, &spec);
    assert!(matches!(
        installed.refuse(package, ApplyOptions::default()),
        UpdateError::Signature(SignatureError::ManifestMismatch { .. })
    ));

    // Rewriting the signature's digest too leaves a signature that does not verify.
    let (dir, spec) = installed.source("forged");
    edit_manifest(&dir, |manifest| {
        manifest["rustc"] = "rustc (attacker)".into()
    });
    let sig_path = dir.join(SIGNATURE_FILE_NAME);
    let mut sig: Value = serde_json::from_slice(&fs::read(&sig_path).unwrap()).unwrap();
    sig["manifest_sha256"] =
        aidex_bundle::sha256_hex(&fs::read(dir.join(MANIFEST_FILE_NAME)).unwrap()).into();
    fs::write(&sig_path, sig.to_string()).unwrap();
    let package = installed.fixture.archive("forged", &dir, &spec);
    let error = installed.refuse(package, ApplyOptions::default());
    assert!(
        matches!(
            error,
            UpdateError::Signature(SignatureError::BadSignature { .. })
        ),
        "{error}"
    );
    assert_eq!(error.reason(), "bad_signature");
    assert!(error.is_integrity());
}

#[test]
fn unknown_revoked_and_missing_signatures_are_refused() {
    let installed = Installed::new();
    let spec = Spec::new("0.2.0");
    let dir = installed.fixture.bundle_dir("stranger", &spec, false);
    common::sign(&dir, &KeyPair::generate("stranger").unwrap());
    let package = installed.fixture.archive("stranger", &dir, &spec);
    assert!(matches!(
        installed.refuse(package, ApplyOptions::default()),
        UpdateError::Signature(SignatureError::UnknownKey { .. })
    ));

    let unsigned_dir = installed.fixture.bundle_dir("unsigned", &spec, false);
    let unsigned = installed.fixture.archive("unsigned", &unsigned_dir, &spec);
    assert!(matches!(
        installed.refuse(unsigned.clone(), ApplyOptions::default()),
        UpdateError::Signature(SignatureError::Unsigned)
    ));

    let mut list = RevocationList::default();
    list.add(Revocation {
        key_id: installed.fixture.key.key_id().into(),
        revoked_at: None,
        reason: Some("leaked".into()),
    });
    list.store(&revocations_path(&installed.fixture.paths()))
        .unwrap();
    let package = installed.fixture.package("revoked", &spec);
    assert!(matches!(
        installed.refuse(package, ApplyOptions::default()),
        UpdateError::Signature(SignatureError::RevokedKey { .. })
    ));
    fs::remove_file(revocations_path(&installed.fixture.paths())).unwrap();

    // The explicit development opt-in accepts the unsigned package, with a warning.
    let report = installed
        .fixture
        .updater()
        .apply(
            &UpdateSource::LocalFile(unsigned),
            &ApplyOptions {
                allow_unsigned: true,
                ..ApplyOptions::default()
            },
        )
        .unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("UNSIGNED"))
    );
    let verified = installed
        .fixture
        .events
        .events()
        .into_iter()
        .rev()
        .find_map(|envelope| match envelope.event {
            aidex_events::EventKind::UpdateVerified {
                unsigned_allowed,
                signer_key_id,
                ..
            } => Some((unsigned_allowed, signer_key_id)),
            _ => None,
        })
        .unwrap();
    assert_eq!(verified, (true, None));
}

#[test]
fn a_tampered_or_incomplete_package_is_refused() {
    let installed = Installed::new();
    let (dir, spec) = installed.source("tampered");
    let cli = dir.join(layout::bin_path("aidex", spec.target));
    fs::write(&cli, "swapped binary").unwrap();
    let package = installed.raw("tampered", &dir, &spec);
    assert!(matches!(
        installed.refuse(package, ApplyOptions::default()),
        UpdateError::Tampered { .. }
    ));

    let (dir, spec) = installed.source("missing");
    fs::remove_file(dir.join(layout::bin_path("aidexd", spec.target))).unwrap();
    let package = installed.raw("missing", &dir, &spec);
    let error = installed.refuse(package, ApplyOptions::default());
    assert!(
        matches!(&error, UpdateError::MissingCriticalFile { path } if path.contains("aidexd")),
        "{error}"
    );

    let (dir, spec) = installed.source("planted");
    fs::write(dir.join("bin").join("version.dll"), "planted").unwrap();
    let package = installed.raw("planted", &dir, &spec);
    assert!(matches!(
        installed.refuse(package, ApplyOptions::default()),
        UpdateError::Tampered { .. }
    ));
}

#[test]
fn wrong_platform_and_incompatible_schema_are_refused() {
    let installed = Installed::new();
    let spec = Spec::new("0.2.0").target(common::other_target());
    let package = installed.fixture.package("foreign", &spec);
    assert!(matches!(
        installed.refuse(package, ApplyOptions::default()),
        UpdateError::WrongTarget { .. }
    ));
    let spec = Spec::new("0.2.0").min_drive_schema(aidex_drive::CURRENT_SCHEMA_VERSION + 1);
    let package = installed.fixture.package("future", &spec);
    let error = installed.refuse(package, ApplyOptions::default());
    assert!(
        matches!(error, UpdateError::IncompatibleDriveSchema { .. }),
        "{error}"
    );
}

#[test]
fn downgrades_are_refused_unless_returning_to_the_recorded_previous() {
    let installed = Installed::new();
    let fixture = &installed.fixture;
    fixture.install("0.2.0");
    let older = fixture.package("older", &Spec::new("0.0.9"));
    let pointer = fixture.pointer_bytes();
    assert!(matches!(
        fixture.updater().apply(
            &UpdateSource::LocalFile(older.clone()),
            &ApplyOptions::default()
        ),
        Err(UpdateError::Downgrade { .. })
    ));
    let allow = ApplyOptions {
        allow_downgrade: true,
        ..ApplyOptions::default()
    };
    assert!(matches!(
        fixture
            .updater()
            .apply(&UpdateSource::LocalFile(older), &allow),
        Err(UpdateError::DowngradeNotPrevious { .. })
    ));
    // The previous version without the flag is still a downgrade.
    let v1 = installed.v1.clone();
    assert!(matches!(
        fixture.updater().apply(
            &UpdateSource::LocalFile(v1.clone()),
            &ApplyOptions::default()
        ),
        Err(UpdateError::Downgrade { .. })
    ));
    assert_eq!(fixture.pointer_bytes(), pointer);
    let report = fixture
        .updater()
        .apply(&UpdateSource::LocalFile(v1), &allow)
        .unwrap();
    assert_eq!(
        report.action,
        aidex_updater::UpdateAction::DowngradeToPrevious
    );
    assert_eq!(fixture.active_version().as_deref(), Some("0.1.0"));
    let entry = fixture.pointer().unwrap().get(host()).cloned().unwrap();
    assert_eq!(entry.previous.unwrap().version, "0.2.0");
}

#[test]
fn hostile_and_oversized_archives_are_refused() {
    let installed = Installed::new();
    let folder = installed.fixture.out.path().join("hostile");
    fs::create_dir_all(&folder).unwrap();
    for (name, entries) in [
        ("traversal", vec![("root/../../evil.txt", &b"x"[..])]),
        ("absolute", vec![("/etc/evil", &b"x"[..])]),
        ("outside", vec![("evil.txt", &b"x"[..])]),
    ] {
        let package = folder.join(format!("{name}.zip"));
        zip_entries(&package, &entries);
        let error = installed.refuse(package, ApplyOptions::default());
        assert!(
            matches!(error, UpdateError::UnsafeArchive { .. }),
            "{name}: {error}"
        );
    }

    let package = installed.fixture.package("big", &Spec::new("0.2.0"));
    let tiny = UpdaterConfig::new(installed.fixture.trust()).with_limits(UpdateLimits {
        max_archive_bytes: 64,
        ..UpdateLimits::default()
    });
    let pointer = installed.fixture.pointer_bytes();
    assert!(matches!(
        installed.fixture.updater_with(tiny).apply(
            &UpdateSource::LocalFile(package.clone()),
            &ApplyOptions::default()
        ),
        Err(UpdateError::ArchiveTooLarge { .. })
    ));
    let mut limits = UpdateLimits::default();
    limits.extract.max_total_bytes = 64;
    let small = UpdaterConfig::new(installed.fixture.trust()).with_limits(limits);
    assert!(matches!(
        installed
            .fixture
            .updater_with(small)
            .apply(&UpdateSource::LocalFile(package), &ApplyOptions::default()),
        Err(UpdateError::ArchiveTooLarge { .. })
    ));
    assert_eq!(installed.fixture.pointer_bytes(), pointer);
    assert!(installed.fixture.staging_entries().is_empty());
}

#[test]
fn a_checksum_mismatch_and_a_failed_self_test_are_refused() {
    let installed = Installed::new();
    let package = installed.fixture.package("sums", &Spec::new("0.2.0"));
    let sums = package.parent().unwrap().join("SHA256SUMS");
    let name = package.file_name().unwrap().to_str().unwrap().to_owned();
    fs::write(&sums, format!("{}  {name}\n", "0".repeat(64))).unwrap();
    assert!(matches!(
        installed.refuse(package.clone(), ApplyOptions::default()),
        UpdateError::ChecksumMismatch { .. }
    ));
    fs::write(&sums, format!("{}  other.zip\n", "0".repeat(64))).unwrap();
    assert!(matches!(
        installed.refuse(package.clone(), ApplyOptions::default()),
        UpdateError::ChecksumMissing { .. }
    ));
    fs::remove_file(&sums).unwrap();

    let failing = UpdaterConfig::new(installed.fixture.trust()).with_self_test(
        std::sync::Arc::new(SelfTestFn(
            |_: &std::path::Path, _: &aidex_bundle::BundleManifest, _: &std::path::Path| {
                Err(UpdateError::SelfTestFailed {
                    command: "aidexd --version".into(),
                    detail: "exit 3".into(),
                })
            },
        )),
    );
    let pointer = installed.fixture.pointer_bytes();
    let versions = installed.fixture.version_dirs();
    let error = installed
        .fixture
        .updater_with(failing)
        .apply(&UpdateSource::LocalFile(package), &ApplyOptions::default())
        .unwrap_err();
    assert!(
        matches!(error, UpdateError::SelfTestFailed { .. }),
        "{error}"
    );
    assert_eq!(installed.fixture.pointer_bytes(), pointer);
    assert_eq!(installed.fixture.version_dirs(), versions);
    assert!(installed.fixture.staging_entries().is_empty());
}
