//! Installing bundles onto a drive: atomicity, launcher placement, and refusal rules.

mod common;

use std::fs;

use aidex_bundle::{
    BundleError, BundleErrorKind, CurrentPointer, InstallSource, InstallStep, LAUNCHER_MARKER,
    PlacementAction, Target, VerifyScope, install, install_with, layout, resolve_active,
};
use aidex_drive::DrivePaths;

fn install_dir(
    dir: &std::path::Path,
    drive: &std::path::Path,
) -> Result<aidex_bundle::InstallReport, BundleError> {
    install(&InstallSource::Directory(dir.to_path_buf()), drive)
}

#[test]
fn fresh_install_commits_the_bundle_places_the_launcher_and_switches_the_pointer() {
    let drive = common::temp_drive();
    let target = common::host();
    let (_source, dir) = common::bundle(target, "0.1.0", "a");
    let report = install_dir(&dir, drive.path()).unwrap();

    let name = format!("0.1.0-{target}");
    assert_eq!(report.dir, format!("versions/{name}"));
    assert!(!report.reused_existing);
    assert_eq!(report.previous, None);
    assert_eq!(
        common::version_dirs(drive.path()),
        std::slice::from_ref(&name)
    );
    assert_eq!(common::staging_leftovers(drive.path()), 0);

    let paths = DrivePaths::open(drive.path()).unwrap();
    let pointer = CurrentPointer::load(&layout::pointer_path(&paths))
        .unwrap()
        .unwrap();
    let entry = pointer.get(target).unwrap();
    assert_eq!(
        (entry.version.as_str(), entry.dir.as_str()),
        ("0.1.0", report.dir.as_str())
    );
    assert_eq!(entry.manifest_sha256, report.manifest_sha256);

    let launcher = layout::launcher_path(&paths, target);
    assert!(
        fs::read_to_string(&launcher)
            .unwrap()
            .contains(LAUNCHER_MARKER)
    );
    assert_eq!(report.launchers[0].action, PlacementAction::Created);
    if !target.is_windows() {
        let script = fs::read_to_string(drive.path().join("aidex")).unwrap();
        assert!(script.starts_with("#!/bin/sh"));
        assert_eq!(report.launchers.len(), 2);
    }

    for scope in [VerifyScope::Critical, VerifyScope::All] {
        let active = resolve_active(&paths, target, scope, &common::unsigned()).unwrap();
        assert_eq!(active.bundle.manifest.version, "0.1.0");
        assert_eq!(active.bundle.scope, scope);
    }
}

#[test]
fn unlisted_source_files_are_not_copied() {
    let drive = common::temp_drive();
    let (_source, dir) = common::bundle(common::host(), "0.1.0", "a");
    fs::write(dir.join("bin").join("injected.dll"), "not listed").unwrap();
    let report = install_dir(&dir, drive.path()).unwrap();
    let installed = drive.path().join("app").join(&report.dir);
    assert!(!installed.join("bin").join("injected.dll").exists());
}

#[test]
fn reinstalling_the_same_build_reuses_the_verified_copy() {
    let drive = common::temp_drive();
    let (_source, dir) = common::bundle(common::host(), "0.1.0", "a");
    install_dir(&dir, drive.path()).unwrap();
    let before = common::pointer_bytes(drive.path()).unwrap();
    let again = install_dir(&dir, drive.path()).unwrap();
    assert!(again.reused_existing);
    assert_eq!(again.previous, None);
    assert!(
        again
            .launchers
            .iter()
            .all(|launcher| launcher.action == PlacementAction::Unchanged)
    );
    assert_eq!(common::pointer_bytes(drive.path()).unwrap(), before);
    assert_eq!(common::version_dirs(drive.path()).len(), 1);
}

#[test]
fn a_different_build_of_the_same_version_gets_its_own_directory() {
    let drive = common::temp_drive();
    let target = common::host();
    let (_a, first) = common::bundle(target, "0.1.0", "a");
    let (_b, second) = common::bundle(target, "0.1.0", "b");
    let one = install_dir(&first, drive.path()).unwrap();
    let two = install_dir(&second, drive.path()).unwrap();
    assert_ne!(one.dir, two.dir);
    assert!(two.dir.starts_with(&format!("versions/0.1.0-{target}-")));
    assert_eq!(
        two.previous.as_ref().map(|p| p.dir.as_str()),
        Some(one.dir.as_str())
    );
    assert_eq!(common::version_dirs(drive.path()).len(), 2);

    let paths = DrivePaths::open(drive.path()).unwrap();
    let active = resolve_active(&paths, target, VerifyScope::All, &common::unsigned()).unwrap();
    assert_eq!(active.bundle.manifest_sha256, two.manifest_sha256);
    let pointer = CurrentPointer::load(&layout::pointer_path(&paths))
        .unwrap()
        .unwrap();
    assert_eq!(
        pointer.get(target).unwrap().previous.as_ref().unwrap().dir,
        one.dir
    );
    // The replaced launcher is kept next to the new one.
    let launcher = layout::launcher_path(&paths, target);
    let previous = std::path::PathBuf::from(format!("{}.previous", launcher.display()));
    assert!(fs::read_to_string(previous).unwrap().contains(" a "));
    assert!(fs::read_to_string(launcher).unwrap().contains(" b "));
    assert_eq!(two.launchers[0].action, PlacementAction::Replaced);
}

#[test]
fn an_interruption_before_the_pointer_switch_keeps_the_previous_bundle_active() {
    let target = common::host();
    let (_a, first) = common::bundle(target, "0.1.0", "a");
    let (_b, second) = common::bundle(target, "0.2.0", "b");
    for step in InstallStep::ALL {
        let drive = common::temp_drive();
        install_dir(&first, drive.path()).unwrap();
        let before = common::pointer_bytes(drive.path()).unwrap();
        let mut observer = |current: InstallStep| {
            if current == step {
                Err(BundleError::Aborted(current))
            } else {
                Ok(())
            }
        };
        let result = install_with(
            &InstallSource::Directory(second.clone()),
            drive.path(),
            &mut observer,
        );
        let error = result.unwrap_err();
        assert_eq!(error.kind(), BundleErrorKind::Aborted, "{step}");
        assert_eq!(common::staging_leftovers(drive.path()), 0, "{step}");

        let paths = DrivePaths::open(drive.path()).unwrap();
        let active = resolve_active(&paths, target, VerifyScope::All, &common::unsigned()).unwrap();
        if step == InstallStep::Cleanup {
            // The switch already happened; only cleanup was skipped.
            assert_eq!(active.bundle.manifest.version, "0.2.0");
        } else {
            assert_eq!(
                common::pointer_bytes(drive.path()).unwrap(),
                before,
                "pointer changed when stopped before {step}"
            );
            assert_eq!(active.bundle.manifest.version, "0.1.0", "{step}");
        }
        // Retrying after the interruption completes the update.
        install_dir(&second, drive.path()).unwrap();
        let active = resolve_active(&paths, target, VerifyScope::All, &common::unsigned()).unwrap();
        assert_eq!(active.bundle.manifest.version, "0.2.0", "{step}");
    }
}

#[test]
fn a_tampered_source_is_rejected_before_anything_is_committed() {
    let drive = common::temp_drive();
    let target = common::host();
    let (_source, dir) = common::bundle(target, "0.1.0", "a");
    fs::write(
        dir.join(layout::bin_path(layout::DAEMON_BINARY, target)),
        "evil",
    )
    .unwrap();
    let error = install_dir(&dir, drive.path()).unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::Integrity, "{error}");
    assert!(error.to_string().contains("aidexd"), "{error}");
    assert!(common::pointer_bytes(drive.path()).is_none());
    assert!(common::version_dirs(drive.path()).is_empty());
    assert_eq!(common::staging_leftovers(drive.path()), 0);
}

#[test]
fn a_bundle_whose_launcher_lacks_the_marker_is_rejected() {
    let drive = common::temp_drive();
    let target = common::host();
    let (_source, dir) = common::bundle(target, "0.1.0", "a");
    let launcher = dir.join(layout::bin_path(layout::LAUNCHER_BINARY, target));
    fs::write(&launcher, "unmarked launcher").unwrap();
    let mut manifest = aidex_bundle::read_manifest(&dir).unwrap().0;
    for file in &mut manifest.files {
        if file.role == aidex_bundle::FileRole::Launcher {
            *file = aidex_bundle::BundleFile::describe(&dir, &file.path, file.role, true).unwrap();
        }
    }
    common::write_manifest(&dir, &manifest);
    let error = install_dir(&dir, drive.path()).unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::InvalidInput, "{error}");
    assert!(common::version_dirs(drive.path()).is_empty());
}

#[test]
fn user_files_at_the_drive_root_are_never_replaced() {
    for target in [Target::WindowsX64, Target::LinuxX64] {
        let root_name = if target.is_windows() {
            "AiDEX.exe"
        } else {
            "aidex"
        };
        for occupied in [root_name.to_owned(), format!("{root_name}.previous")] {
            let drive = common::temp_drive();
            let user_file = drive.path().join(&occupied);
            fs::write(&user_file, "user data").unwrap();
            let (_source, dir) = common::bundle(target, "0.1.0", "a");
            let error = install_dir(&dir, drive.path()).unwrap_err();
            assert_eq!(
                error.kind(),
                BundleErrorKind::Occupied,
                "{occupied}: {error}"
            );
            assert_eq!(fs::read_to_string(&user_file).unwrap(), "user data");
            assert!(common::pointer_bytes(drive.path()).is_none(), "{occupied}");
            assert!(common::version_dirs(drive.path()).is_empty(), "{occupied}");
        }
    }
}

#[test]
fn bundles_for_several_targets_share_one_drive() {
    let drive = common::temp_drive();
    let (_w, windows) = common::bundle(Target::WindowsX64, "0.1.0", "w");
    let (_l, linux) = common::bundle(Target::LinuxX64, "0.1.0", "l");
    let (_m, mac) = common::bundle(Target::MacosArm64, "0.1.0", "m");
    for dir in [&windows, &linux, &mac] {
        install_dir(dir, drive.path()).unwrap();
    }
    let paths = DrivePaths::open(drive.path()).unwrap();
    let pointer = CurrentPointer::load(&layout::pointer_path(&paths))
        .unwrap()
        .unwrap();
    assert_eq!(
        pointer.installed_targets(),
        ["linux-x64", "macos-arm64", "windows-x64"]
    );
    assert!(drive.path().join("AiDEX.exe").is_file());
    assert!(drive.path().join("aidex").is_file());
    assert!(
        drive
            .path()
            .join("app/launcher/linux-x64/aidex-launcher")
            .is_file()
    );
    assert!(
        drive
            .path()
            .join("app/launcher/macos-arm64/aidex-launcher")
            .is_file()
    );
    for target in [Target::WindowsX64, Target::LinuxX64, Target::MacosArm64] {
        let active = resolve_active(&paths, target, VerifyScope::All, &common::unsigned()).unwrap();
        assert_eq!(active.bundle.manifest.target, target);
    }
}

#[test]
fn an_unreadable_pointer_is_reported_and_left_untouched() {
    let drive = common::temp_drive();
    let pointer = drive.path().join("app").join("current.json");
    fs::write(&pointer, "{ not json").unwrap();
    let (_source, dir) = common::bundle(common::host(), "0.1.0", "a");
    let error = install_dir(&dir, drive.path()).unwrap_err();
    assert!(matches!(error, BundleError::Pointer { .. }), "{error}");
    assert_eq!(fs::read_to_string(&pointer).unwrap(), "{ not json");
}

#[test]
fn installing_requires_an_initialized_drive() {
    let folder = tempfile::tempdir().unwrap();
    let (_source, dir) = common::bundle(common::host(), "0.1.0", "a");
    let error = install_dir(&dir, folder.path()).unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::Drive, "{error}");
    assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
}

#[test]
fn a_damaged_installed_copy_is_not_reused() {
    let drive = common::temp_drive();
    let target = common::host();
    let (_source, dir) = common::bundle(target, "0.1.0", "a");
    let first = install_dir(&dir, drive.path()).unwrap();
    let installed = drive.path().join("app").join(&first.dir);
    fs::write(
        installed.join(layout::bin_path(layout::CLI_BINARY, target)),
        "corrupt",
    )
    .unwrap();
    let repaired = install_dir(&dir, drive.path()).unwrap();
    assert!(!repaired.reused_existing);
    assert_ne!(repaired.dir, first.dir);
    let paths = DrivePaths::open(drive.path()).unwrap();
    resolve_active(&paths, target, VerifyScope::All, &common::unsigned()).unwrap();
}
