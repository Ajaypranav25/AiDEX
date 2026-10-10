//! Synthetic bundles and drives for integration tests.

#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use aidex_bundle::{
    BundleFile, BundleManifest, FileRole, LAUNCHER_MARKER, LicenseEntry, MANIFEST_FILE_NAME,
    MANIFEST_SCHEMA_VERSION, PRODUCT_NAME, StatePaths, Target, layout,
};
use aidex_core::Timestamp;

/// A temporary initialized drive (removable check skipped, as for developer folders).
pub fn temp_drive() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    aidex_drive::init_with(
        root.path(),
        aidex_drive::InitOptions {
            require_removable: false,
        },
    )
    .unwrap();
    root
}

/// The host target, or Windows when the host has no target name.
pub fn host() -> Target {
    Target::host().unwrap_or(Target::WindowsX64)
}

/// A target other than `target`.
pub fn other_than(target: Target) -> Target {
    if target == Target::LinuxX64 {
        Target::WindowsX64
    } else {
        Target::LinuxX64
    }
}

/// Write a complete synthetic bundle into `dir`; `tag` makes builds differ.
pub fn write_bundle(dir: &Path, target: Target, version: &str, tag: &str) -> BundleManifest {
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::create_dir_all(dir.join("licenses/aidex")).unwrap();
    let mut files = Vec::new();
    let binaries = [
        (FileRole::Launcher, layout::LAUNCHER_BINARY),
        (FileRole::Cli, layout::CLI_BINARY),
        (FileRole::Daemon, layout::DAEMON_BINARY),
        (FileRole::Helper, layout::HELPER_BINARY),
    ];
    for (role, stem) in binaries {
        let path = layout::bin_path(stem, target);
        let marker = if role == FileRole::Launcher {
            LAUNCHER_MARKER
        } else {
            ""
        };
        fs::write(
            dir.join(&path),
            format!("{stem} {version} {tag} {target}\n{marker}\n"),
        )
        .unwrap();
        files.push(BundleFile::describe(dir, &path, role, true).unwrap());
    }
    let license = "licenses/aidex/LICENSE";
    fs::write(dir.join(license), "MIT License\n").unwrap();
    files.push(BundleFile::describe(dir, license, FileRole::Doc, false).unwrap());
    let mut manifest = BundleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        product: PRODUCT_NAME.into(),
        version: version.into(),
        git_commit: Some("0".repeat(40)),
        source_dirty: Some(false),
        target,
        rust_target: target.default_rust_triple().into(),
        rustc: Some("rustc 1.95.0 (test)".into()),
        created_at: Timestamp::from_unix_millis(1_760_000_000_000).unwrap(),
        min_drive_schema: aidex_drive::CURRENT_SCHEMA_VERSION,
        state: StatePaths::current(),
        files,
        licenses: vec![LicenseEntry {
            component: "aidex-cli".into(),
            version: version.into(),
            spdx: "MIT".into(),
            source: "workspace".into(),
            path: Some(license.into()),
        }],
    };
    manifest.normalize();
    write_manifest(dir, &manifest);
    manifest
}

pub fn write_manifest(dir: &Path, manifest: &BundleManifest) {
    fs::write(dir.join(MANIFEST_FILE_NAME), manifest.to_json().unwrap()).unwrap();
}

/// A fresh bundle directory inside a new temporary directory.
pub fn bundle(target: Target, version: &str, tag: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join(format!("aidex-{version}-{target}"));
    write_bundle(&dir, target, version, tag);
    (temp, dir)
}

/// The drive's `app/current.json` bytes, if any.
pub fn pointer_bytes(drive: &Path) -> Option<Vec<u8>> {
    fs::read(drive.join("app").join("current.json")).ok()
}

/// Names under `app/versions/`.
pub fn version_dirs(drive: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(drive.join("app").join("versions"))
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Entries left under `app/staging/`.
pub fn staging_leftovers(drive: &Path) -> usize {
    fs::read_dir(drive.join("app").join("staging"))
        .map(|entries| entries.count())
        .unwrap_or(0)
}
