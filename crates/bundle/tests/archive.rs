//! Deterministic archives and fail-closed extraction (`archive` feature).

mod common;

use std::{fs, io::Write, path::Path};

use aidex_bundle::{
    BundleErrorKind, InstallSource, Target, VerifyScope,
    archive::{ArchiveFormat, ExtractLimits, archive_file_name, extract_archive, write_archive},
    install, resolve_active, verify_bundle,
};
use aidex_drive::DrivePaths;

fn build_archive(target: Target, out_dir: &Path) -> (tempfile::TempDir, std::path::PathBuf) {
    let (source, dir) = common::bundle(target, "0.1.0", "a");
    let bundle = verify_bundle(&dir, VerifyScope::All).unwrap();
    let out = out_dir.join(archive_file_name("0.1.0", target));
    write_archive(&bundle, &out, ArchiveFormat::for_target(target)).unwrap();
    (source, out)
}

#[test]
fn archives_are_deterministic_and_install_onto_a_drive() {
    for target in [Target::WindowsX64, Target::LinuxX64] {
        let out = tempfile::tempdir().unwrap();
        let (_source, dir) = common::bundle(target, "0.1.0", "a");
        let bundle = verify_bundle(&dir, VerifyScope::All).unwrap();
        let format = ArchiveFormat::for_target(target);
        let first = write_archive(&bundle, &out.path().join("first"), format).unwrap();
        let second = write_archive(&bundle, &out.path().join("second"), format).unwrap();
        assert_eq!(first.sha256, second.sha256, "{target}");
        assert_eq!(
            fs::read(&first.path).unwrap(),
            fs::read(&second.path).unwrap()
        );
        assert_eq!(first.entries, bundle.manifest.files.len() + 2);

        let archive = out.path().join(archive_file_name("0.1.0", target));
        fs::rename(&first.path, &archive).unwrap();
        assert!(matches!(
            InstallSource::from_path(&archive).unwrap(),
            InstallSource::Archive(_)
        ));
        let drive = common::temp_drive();
        let report = install(&InstallSource::from_path(&archive).unwrap(), drive.path()).unwrap();
        assert_eq!(report.target, target);
        assert_eq!(common::staging_leftovers(drive.path()), 0);
        let paths = DrivePaths::open(drive.path()).unwrap();
        resolve_active(&paths, target, VerifyScope::All, &common::unsigned()).unwrap();
    }
}

#[test]
fn signed_archives_carry_bundle_sig_through_extraction_and_install() {
    let key = aidex_bundle::KeyPair::generate("archive-tests").unwrap();
    let target = common::host();
    let (_source, dir) = common::bundle(target, "0.1.0", "a");
    common::sign(&dir, &key);
    let bundle = verify_bundle(&dir, VerifyScope::All).unwrap();
    let out = tempfile::tempdir().unwrap();
    let archive = out.path().join(archive_file_name("0.1.0", target));
    let summary = write_archive(&bundle, &archive, ArchiveFormat::for_target(target)).unwrap();
    assert_eq!(summary.entries, bundle.manifest.files.len() + 3);
    let destination = tempfile::tempdir().unwrap();
    let extracted =
        extract_archive(&archive, destination.path(), &ExtractLimits::default()).unwrap();
    assert_eq!(
        aidex_bundle::read_signature(&extracted).unwrap(),
        aidex_bundle::read_signature(&dir).unwrap()
    );
    let drive = common::temp_drive();
    let trust = aidex_bundle::TrustStore::empty()
        .with_key(key.trusted_key(aidex_bundle::KeySource::Explicit));
    let policy = aidex_bundle::SignaturePolicy::require(trust);
    aidex_bundle::install_signed(
        &InstallSource::from_path(&archive).unwrap(),
        drive.path(),
        &policy,
        &mut |_| Ok(()),
    )
    .unwrap();
    let paths = DrivePaths::open(drive.path()).unwrap();
    let active = resolve_active(&paths, target, VerifyScope::All, &policy).unwrap();
    assert!(active.signature.is_signed());
}

#[test]
fn unsafe_entries_and_oversized_archives_have_typed_errors() {
    let out = tempfile::tempdir().unwrap();
    let archive = out.path().join("traversal.zip");
    zip_with(&archive, &[("root/../../evil.txt", b"x")]);
    assert!(matches!(
        extract(&archive).unwrap_err(),
        aidex_bundle::BundleError::ArchiveEntry { .. }
    ));
    let (_source, archive) = build_archive(Target::WindowsX64, out.path());
    let destination = tempfile::tempdir().unwrap();
    let tiny = ExtractLimits {
        max_entries: 16_384,
        max_total_bytes: 64,
        max_file_bytes: 64,
    };
    assert!(matches!(
        extract_archive(&archive, destination.path(), &tiny).unwrap_err(),
        aidex_bundle::BundleError::ArchiveLimit(_)
    ));
}

#[test]
fn archive_names_follow_the_target() {
    assert_eq!(
        archive_file_name("0.1.0", Target::WindowsX64),
        "aidex-0.1.0-windows-x64.zip"
    );
    assert_eq!(
        archive_file_name("0.1.0", Target::MacosArm64),
        "aidex-0.1.0-macos-arm64.tar.gz"
    );
    assert_eq!(
        ArchiveFormat::detect(Path::new("x.TGZ")),
        Some(ArchiveFormat::TarGz)
    );
    assert_eq!(ArchiveFormat::detect(Path::new("x.rar")), None);
}

#[test]
fn a_tampered_archive_fails_verification_and_installs_nothing() {
    let out = tempfile::tempdir().unwrap();
    let (_source, dir) = common::bundle(Target::LinuxX64, "0.1.0", "a");
    // Tamper after the manifest was written, then archive the bundle by hand.
    fs::write(dir.join("bin/aidexd"), "evil").unwrap();
    let archive = out.path().join("aidex-0.1.0-linux-x64.tar.gz");
    write_tar(&archive, &dir);
    let drive = common::temp_drive();
    let error = install(&InstallSource::Archive(archive), drive.path()).unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::Integrity, "{error}");
    assert!(common::pointer_bytes(drive.path()).is_none());
    assert!(common::version_dirs(drive.path()).is_empty());
    assert_eq!(common::staging_leftovers(drive.path()), 0);
}

fn write_tar(out: &Path, dir: &Path) {
    let file = fs::File::create(out).unwrap();
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    builder
        .append_dir_all("aidex-0.1.0-linux-x64", dir)
        .unwrap();
    builder.into_inner().unwrap().finish().unwrap();
}

fn zip_with(out: &Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(out).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    for (name, data) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap();
}

fn extract(archive: &Path) -> Result<std::path::PathBuf, aidex_bundle::BundleError> {
    let destination = tempfile::tempdir().unwrap();
    extract_archive(archive, destination.path(), &ExtractLimits::default())
}

#[test]
fn hostile_zip_entries_are_refused() {
    let out = tempfile::tempdir().unwrap();
    type Entries<'a> = Vec<(&'a str, &'a [u8])>;
    let cases: Vec<(&str, Entries)> = vec![
        ("traversal", vec![("root/../../evil.txt", b"x")]),
        ("absolute", vec![("/etc/evil", b"x")]),
        ("backslash", vec![("root\\..\\evil", b"x")]),
        ("top-level file", vec![("evil.txt", b"x")]),
        (
            "two roots",
            vec![("a/bundle.json", b"{}"), ("b/bundle.json", b"{}")],
        ),
        (
            "case duplicate",
            vec![("root/bin/aidex", b"1"), ("root/BIN/AIDEX", b"2")],
        ),
        ("device name", vec![("root/bin/CON", b"x")]),
        ("no manifest", vec![("root/bin/aidex", b"x")]),
    ];
    for (name, entries) in cases {
        let archive = out.path().join(format!("{}.zip", name.replace(' ', "-")));
        zip_with(&archive, &entries);
        let error = extract(&archive).unwrap_err();
        assert_eq!(
            error.kind(),
            BundleErrorKind::InvalidInput,
            "{name}: {error}"
        );
    }
}

#[test]
fn links_in_archives_are_refused() {
    let out = tempfile::tempdir().unwrap();
    let zip_path = out.path().join("link.zip");
    let file = fs::File::create(&zip_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .add_symlink(
            "root/bin/aidex",
            "/usr/bin/sh",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer.finish().unwrap();
    assert_eq!(
        extract(&zip_path).unwrap_err().kind(),
        BundleErrorKind::InvalidInput
    );

    let tar_path = out.path().join("link.tar.gz");
    let encoder = flate2::write::GzEncoder::new(
        fs::File::create(&tar_path).unwrap(),
        flate2::Compression::fast(),
    );
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    builder
        .append_link(&mut header, "root/bin/aidex", "/usr/bin/sh")
        .unwrap();
    builder.into_inner().unwrap().finish().unwrap();
    let error = extract(&tar_path).unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::InvalidInput, "{error}");
}

#[test]
fn extraction_limits_are_enforced() {
    let out = tempfile::tempdir().unwrap();
    let (_source, archive) = build_archive(Target::WindowsX64, out.path());
    let destination = tempfile::tempdir().unwrap();
    let tiny = ExtractLimits {
        max_entries: 16_384,
        max_total_bytes: 64,
        max_file_bytes: 64,
    };
    let error = extract_archive(&archive, destination.path(), &tiny).unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::InvalidInput, "{error}");
    let few = ExtractLimits {
        max_entries: 2,
        ..ExtractLimits::default()
    };
    let destination = tempfile::tempdir().unwrap();
    assert!(extract_archive(&archive, destination.path(), &few).is_err());
}
