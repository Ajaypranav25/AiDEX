use super::*;

fn temp_root(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "aidex-drive-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn recognizes_drive_roots_and_resolves_explicit_directory_roots() {
    assert!(is_windows_drive_root(Path::new(r"H:\")));
    assert!(is_windows_drive_root(Path::new(r"\\?\H:\")));
    assert!(is_windows_drive_root(Path::new("h:/")));
    assert!(!is_windows_drive_root(Path::new(r"H:\folder")));

    let root = temp_root("root");
    assert_eq!(
        resolve_root(Some(&root)).unwrap(),
        fs::canonicalize(&root).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn initialization_recovers_after_partial_directories_and_preserves_user_content() {
    let root = temp_root("retry");
    fs::write(root.join("keep.txt"), "user data").unwrap();
    fs::create_dir_all(root.join("runtime")).unwrap();
    fs::write(root.join("runtime/existing.bin"), b"keep me").unwrap();

    initialize_at(&root).unwrap();
    initialize_at(&root).unwrap();

    let manifest =
        DriveManifest::from_json(&fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
            .unwrap();
    for relative in [
        &manifest.layout.public_runtime,
        &manifest.layout.public_models,
        &manifest.layout.private_vault,
        &manifest.layout.private_identity,
        &manifest.layout.private_sessions,
        &manifest.layout.private_config,
    ] {
        assert!(
            relative.resolve_under(&root).is_dir(),
            "missing {}",
            relative.as_str()
        );
    }
    assert_eq!(
        fs::read_to_string(root.join("keep.txt")).unwrap(),
        "user data"
    );
    assert_eq!(
        fs::read(root.join("runtime/existing.bin")).unwrap(),
        b"keep me"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_reports_missing_corrupt_and_future_manifests_without_repairing_them() {
    let root = temp_root("status");
    let manifest_path = root.join(MANIFEST_FILE_NAME);
    assert!(matches!(
        status_at(&root, true, Some(20)),
        Err(DriveError::MissingManifest(_))
    ));

    for content in ["not json", r#"{"schema_version":2,"layout":{}}"#] {
        fs::write(&manifest_path, content).unwrap();
        let before = fs::read(&manifest_path).unwrap();
        assert!(matches!(
            status_at(&root, true, Some(20)),
            Err(DriveError::CorruptManifest(_, _))
        ));
        assert!(matches!(
            initialize_at(&root),
            Err(DriveError::CorruptManifest(_, _))
        ));
        assert_eq!(fs::read(&manifest_path).unwrap(), before);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn writes_only_the_canonical_root_manifest() {
    let root = temp_root("canonical");
    initialize_at(&root).unwrap();

    assert!(root.join(MANIFEST_FILE_NAME).is_file());
    assert!(!root.join(".aidex").exists());
    assert!(!root.join(".aidex/manifest.toml").exists());
    assert!(
        DriveManifest::from_json(&fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
            .is_ok()
    );
    fs::remove_dir_all(root).unwrap();
}
