//! `resolve_active`: what the launcher refuses before running anything.

mod common;

use std::{fs, path::PathBuf};

use aidex_bundle::{
    BundleError, BundleErrorKind, CurrentPointer, InstallSource, IntegrityProblem, Target,
    VerifyScope, install, layout, resolve_active,
};
use aidex_drive::DrivePaths;
use serde_json::Value;

struct Installed {
    drive: tempfile::TempDir,
    _source: tempfile::TempDir,
    target: Target,
    dir: PathBuf,
}

impl Installed {
    fn new(target: Target) -> Self {
        let drive = common::temp_drive();
        let (source, bundle) = common::bundle(target, "0.1.0", "a");
        let report = install(&InstallSource::Directory(bundle), drive.path()).unwrap();
        let dir = drive.path().join("app").join(&report.dir);
        Self {
            drive,
            _source: source,
            target,
            dir,
        }
    }

    fn paths(&self) -> DrivePaths {
        DrivePaths::open(self.drive.path()).unwrap()
    }

    fn resolve(&self, scope: VerifyScope) -> Result<aidex_bundle::ActiveBundle, BundleError> {
        resolve_active(&self.paths(), self.target, scope)
    }

    fn pointer_file(&self) -> PathBuf {
        layout::pointer_path(&self.paths())
    }

    fn edit_pointer(&self, edit: impl FnOnce(&mut Value)) {
        let mut value: Value =
            serde_json::from_slice(&fs::read(self.pointer_file()).unwrap()).unwrap();
        edit(&mut value["targets"][self.target.as_str()]);
        fs::write(self.pointer_file(), value.to_string()).unwrap();
    }
}

fn problem(error: &BundleError) -> Option<&IntegrityProblem> {
    match error {
        BundleError::Integrity { problem, .. } => Some(problem),
        _ => None,
    }
}

#[test]
fn a_tampered_critical_binary_is_refused() {
    let installed = Installed::new(common::host());
    let daemon = installed
        .dir
        .join(layout::bin_path(layout::DAEMON_BINARY, installed.target));
    let original = fs::read(&daemon).unwrap();
    let mut tampered = original.clone();
    tampered[0] ^= 0x01;
    fs::write(&daemon, &tampered).unwrap();
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::Integrity);
    assert!(
        matches!(
            problem(&error),
            Some(IntegrityProblem::DigestMismatch { .. })
        ),
        "{error}"
    );
    assert!(error.to_string().contains("sha256 mismatch"), "{error}");

    fs::write(&daemon, b"short").unwrap();
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert!(
        matches!(problem(&error), Some(IntegrityProblem::SizeMismatch { .. })),
        "{error}"
    );

    fs::remove_file(&daemon).unwrap();
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert_eq!(problem(&error), Some(&IntegrityProblem::Missing));
}

#[test]
fn doc_files_are_checked_only_by_the_full_scope() {
    let installed = Installed::new(common::host());
    fs::write(installed.dir.join("licenses/aidex/LICENSE"), "changed").unwrap();
    installed.resolve(VerifyScope::Critical).unwrap();
    let error = installed.resolve(VerifyScope::All).unwrap_err();
    assert!(problem(&error).is_some(), "{error}");
}

#[test]
fn an_unlisted_file_beside_the_binaries_is_refused() {
    let installed = Installed::new(common::host());
    fs::write(installed.dir.join("bin").join("version.dll"), "planted").unwrap();
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert_eq!(
        problem(&error),
        Some(&IntegrityProblem::Unlisted),
        "{error}"
    );
}

#[test]
fn a_missing_pointer_or_target_entry_is_reported_as_not_installed() {
    let installed = Installed::new(common::host());
    let other = common::other_than(installed.target);
    let error = resolve_active(&installed.paths(), other, VerifyScope::Critical).unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::NotInstalled);
    assert!(
        error.to_string().contains(installed.target.as_str()),
        "{error}"
    );

    fs::remove_file(installed.pointer_file()).unwrap();
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert!(matches!(error, BundleError::NotInstalled { .. }), "{error}");
}

#[test]
fn a_pointer_escaping_app_versions_is_refused() {
    let installed = Installed::new(common::host());
    for dir in [
        "../../outside",
        "versions/../current.json",
        "/tmp/bundle",
        "versions/a/b",
        "staging/x",
    ] {
        installed.edit_pointer(|entry| entry["dir"] = dir.into());
        let error = installed.resolve(VerifyScope::Critical).unwrap_err();
        assert!(
            matches!(error, BundleError::Pointer { .. }),
            "{dir}: {error}"
        );
        assert_eq!(error.kind(), BundleErrorKind::Integrity);
    }
}

#[test]
fn a_bundle_for_another_target_is_refused() {
    let installed = Installed::new(common::host());
    let other = common::other_than(installed.target);
    let (_source, foreign) = common::bundle(other, "0.1.0", "x");
    let report = install(&InstallSource::Directory(foreign), installed.drive.path()).unwrap();
    // Point the host entry at the other target's bundle.
    installed.edit_pointer(|entry| {
        entry["dir"] = report.dir.clone().into();
        entry["manifest_sha256"] = report.manifest_sha256.clone().into();
    });
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert!(
        matches!(error, BundleError::WrongTarget { expected, found } if expected == installed.target && found == other),
        "{error}"
    );
    assert_eq!(error.kind(), BundleErrorKind::Incompatible);
}

#[test]
fn a_rewritten_manifest_does_not_match_the_pinned_digest() {
    let installed = Installed::new(common::host());
    let (mut manifest, _) = aidex_bundle::read_manifest(&installed.dir).unwrap();
    // Swap in a tampered daemon together with a manifest that matches it.
    let daemon = layout::bin_path(layout::DAEMON_BINARY, installed.target);
    fs::write(installed.dir.join(&daemon), "evil daemon").unwrap();
    for file in &mut manifest.files {
        if file.path == daemon {
            *file = aidex_bundle::BundleFile::describe(&installed.dir, &daemon, file.role, true)
                .unwrap();
        }
    }
    common::write_manifest(&installed.dir, &manifest);
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert!(
        matches!(problem(&error), Some(IntegrityProblem::ManifestPin { .. })),
        "{error}"
    );
}

#[test]
fn a_version_disagreement_between_pointer_and_manifest_is_refused() {
    let installed = Installed::new(common::host());
    installed.edit_pointer(|entry| entry["version"] = "9.9.9".into());
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert!(
        matches!(error, BundleError::VersionMismatch { .. }),
        "{error}"
    );
}

#[test]
fn a_newer_pointer_schema_is_incompatible_not_corrupt() {
    let installed = Installed::new(common::host());
    let mut value: Value =
        serde_json::from_slice(&fs::read(installed.pointer_file()).unwrap()).unwrap();
    value["schema_version"] = 2.into();
    fs::write(installed.pointer_file(), value.to_string()).unwrap();
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::Incompatible, "{error}");
    assert!(CurrentPointer::load(&installed.pointer_file()).is_err());
}

#[cfg(unix)]
#[test]
fn links_inside_the_bundle_are_refused() {
    let installed = Installed::new(common::host());
    let cli = installed
        .dir
        .join(layout::bin_path(layout::CLI_BINARY, installed.target));
    let moved = installed.drive.path().join("real-cli");
    fs::rename(&cli, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &cli).unwrap();
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert_eq!(problem(&error), Some(&IntegrityProblem::Link), "{error}");

    // A version directory replaced by a link is refused too.
    fs::remove_file(&cli).unwrap();
    fs::rename(&moved, &cli).unwrap();
    let elsewhere = installed.drive.path().join("elsewhere");
    fs::rename(&installed.dir, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &installed.dir).unwrap();
    let error = installed.resolve(VerifyScope::Critical).unwrap_err();
    assert_eq!(problem(&error), Some(&IntegrityProblem::Link), "{error}");
}
