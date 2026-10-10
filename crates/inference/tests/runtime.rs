//! Runtime bundle installation: only pinned archives install, nothing escapes
//! the install directory, licence and source are recorded, and installs are
//! verified before launch.

mod support;

use std::fs;

use aidex_core::CancelToken;
use aidex_inference::runtime::{
    ArchiveFormat, MANIFEST_FILE, RuntimeCatalog, install_from_catalog, install_runtime,
    list_installed,
};
use support::{catalog_entry, fake_runtime_archive, files_under, temp_drive, write_zip};

#[test]
fn a_pinned_archive_installs_with_its_manifest_licence_and_source() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let (archive, entry) = fake_runtime_archive(dir.path());
    let installed = install_runtime(&paths, &archive, &entry).unwrap();

    let platform = &entry.platform;
    assert_eq!(
        installed.relative_dir,
        format!("runtime/{platform}/llama/b0-test-cpu")
    );
    let manifest = &installed.manifest;
    assert_eq!(manifest.catalog_id, entry.id);
    assert_eq!(manifest.archive_sha256, entry.sha256);
    assert_eq!(manifest.license, "MIT");
    assert_eq!(manifest.source, entry.source);
    assert_eq!(manifest.url, entry.url);
    assert_eq!(manifest.files.len(), 3);
    assert!(installed.server_path().is_file());
    assert!(installed.dir.join(MANIFEST_FILE).is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(installed.server_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
        let library = fs::metadata(installed.dir.join("ggml-fake.dll")).unwrap();
        assert_eq!(library.permissions().mode() & 0o777, 0o644);
    }
    let catalog = RuntimeCatalog::from_entries(vec![entry.clone()]).unwrap();
    let checked = installed.verify(&catalog, &CancelToken::new()).unwrap();
    assert_eq!(checked.files, 3);

    // Re-installing the same archive is idempotent; listing finds it.
    let again = install_runtime(&paths, &archive, &entry).unwrap();
    assert_eq!(again.manifest.tree_sha256, manifest.tree_sha256);
    let listing = list_installed(&paths).unwrap();
    assert_eq!(listing.runtimes.len(), 1);
    assert!(listing.problems.is_empty());
}

#[test]
fn archives_that_do_not_match_the_catalog_are_refused() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let (archive, entry) = fake_runtime_archive(dir.path());
    let mut bytes = fs::read(&archive).unwrap();
    bytes.push(0);
    let tampered = dir.path().join("tampered.zip");
    fs::write(&tampered, &bytes).unwrap();
    let error = install_runtime(&paths, &tampered, &entry).unwrap_err();
    assert_eq!(error.kind(), "hash_mismatch", "{error}");

    let catalog = RuntimeCatalog::from_entries(vec![entry]).unwrap();
    let error = install_from_catalog(
        &paths,
        &tampered,
        &catalog,
        &mut |_| {},
        &CancelToken::new(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), "runtime_not_in_catalog");
    assert!(error.advice().unwrap().contains("aidex runtime list"));
    // The real catalog does not accept arbitrary archives either.
    let builtin = RuntimeCatalog::builtin().unwrap();
    assert!(
        install_from_catalog(&paths, &archive, &builtin, &mut |_| {}, &CancelToken::new()).is_err()
    );
    assert!(files_under(&paths.runtime()).is_empty());
}

#[test]
fn zip_slip_absolute_paths_links_and_duplicates_are_rejected() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let server = support::server_binary_name();
    type Entry<'a> = (&'a str, &'a [u8], u32);
    let cases: Vec<(&str, Vec<Entry>)> = vec![
        (
            "parent",
            vec![(server.as_str(), b"x", 0o755), ("../evil.txt", b"x", 0o644)],
        ),
        ("nested", vec![("bin/../../evil.txt", b"x", 0o644)]),
        ("absolute", vec![("/tmp/evil.txt", b"x", 0o644)]),
        ("drive", vec![("C:/evil.txt", b"x", 0o644)]),
        ("backslash", vec![("..\\evil.txt", b"x", 0o644)]),
        ("stream", vec![("llama.dll:hidden", b"x", 0o644)]),
        ("device", vec![("nul.dll", b"x", 0o644)]),
        (
            "duplicate",
            vec![("A.dll", b"x", 0o644), ("a.dll", b"y", 0o644)],
        ),
        ("manifest", vec![(MANIFEST_FILE, b"{}", 0o644)]),
    ];
    for (name, entries) in cases {
        let archive = dir.path().join(format!("{name}.zip"));
        write_zip(&archive, &entries);
        let entry = catalog_entry(&archive, ArchiveFormat::Zip);
        let error = install_runtime(&paths, &archive, &entry).unwrap_err();
        assert_eq!(error.kind(), "unsafe_archive", "{name}: {error}");
    }

    // A symbolic link entry.
    let archive = dir.path().join("link.zip");
    {
        let file = fs::File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.add_symlink(
            "link.dll",
            "/etc/passwd",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.finish().unwrap();
    }
    let entry = catalog_entry(&archive, ArchiveFormat::Zip);
    assert_eq!(
        install_runtime(&paths, &archive, &entry)
            .unwrap_err()
            .kind(),
        "unsafe_archive"
    );

    // An archive without the server binary is not a runtime.
    let archive = dir.path().join("empty.zip");
    write_zip(&archive, &[("README", b"nothing here", 0o644)]);
    let entry = catalog_entry(&archive, ArchiveFormat::Zip);
    assert_eq!(
        install_runtime(&paths, &archive, &entry)
            .unwrap_err()
            .kind(),
        "unsafe_archive"
    );

    assert!(!dir.path().join("evil.txt").exists());
    assert!(!paths.runtime().join("evil.txt").exists());
    assert!(
        files_under(&paths.runtime()).is_empty(),
        "failed installs leave no files: {:?}",
        files_under(&paths.runtime())
    );
}

#[test]
fn tar_gz_archives_are_refused_with_a_clear_error() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("llama-linux.tar.gz");
    fs::write(&archive, b"pretend tarball").unwrap();
    let entry = catalog_entry(&archive, ArchiveFormat::TarGz);
    let error = install_runtime(&paths, &archive, &entry).unwrap_err();
    assert_eq!(error.kind(), "unsupported_archive_format");
    assert!(error.advice().unwrap().contains(".zip"));
}

#[test]
fn interrupted_install_staging_is_cleaned_up() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let (archive, entry) = fake_runtime_archive(dir.path());
    let engine = paths.runtime().join(&entry.platform).join("llama");
    fs::create_dir_all(engine.join(".staging-deadbeef/sub")).unwrap();
    fs::write(engine.join(".staging-deadbeef/sub/partial.dll"), b"half").unwrap();
    install_runtime(&paths, &archive, &entry).unwrap();
    assert!(!engine.join(".staging-deadbeef").exists());
    let names: Vec<String> = fs::read_dir(&engine)
        .unwrap()
        .map(|item| item.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["b0-test-cpu"]);
}

#[test]
fn installed_runtimes_are_verified_against_their_manifest_and_catalog() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let (archive, entry) = fake_runtime_archive(dir.path());
    let installed = install_runtime(&paths, &archive, &entry).unwrap();
    let catalog = RuntimeCatalog::from_entries(vec![entry.clone()]).unwrap();
    let cancel = CancelToken::new();

    // A planted library next to the server would be loaded by it.
    let planted = installed.dir.join("version.dll");
    fs::write(&planted, b"planted").unwrap();
    let error = installed.verify(&catalog, &cancel).unwrap_err();
    assert_eq!(error.kind(), "runtime_corrupt");
    assert!(error.to_string().contains("unexpected file"), "{error}");
    fs::remove_file(&planted).unwrap();

    // A modified binary fails its hash.
    let library = installed.dir.join("ggml-fake.dll");
    fs::write(&library, b"NOT a real library").unwrap();
    let error = installed.verify(&catalog, &cancel).unwrap_err();
    assert!(error.to_string().contains("SHA-256"), "{error}");
    fs::write(&library, b"not a real library").unwrap();
    installed.verify(&catalog, &cancel).unwrap();

    // A runtime that is not in this build's catalog is refused.
    let error = installed
        .verify(&RuntimeCatalog::builtin().unwrap(), &cancel)
        .unwrap_err();
    assert!(error.to_string().contains("pinned catalog"), "{error}");

    // A catalog that pins the extracted contents rejects a different tree.
    let mut pinned = entry;
    pinned.tree_sha256 = Some("0".repeat(64));
    let error = installed
        .verify(
            &RuntimeCatalog::from_entries(vec![pinned]).unwrap(),
            &cancel,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("pinned release contents"),
        "{error}"
    );
}
