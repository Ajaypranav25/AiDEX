use super::*;
use crate::{DrivePaths, V1_BACKUP_FILE_NAME};

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

const V1_MANIFEST: &str = r#"{"schema_version":1,"layout":{"public_runtime":"runtime","public_models":"models","private_vault":"private/vault","private_identity":"private/identity","private_sessions":"private/sessions","private_config":"private/config"}}"#;

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
    assert_eq!(manifest.schema_version(), CURRENT_SCHEMA_VERSION);
    for (name, relative) in manifest.layout.entries() {
        assert!(
            relative.resolve_under(&root).is_dir(),
            "missing {name} at {}",
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

    for content in [
        "not json",
        r#"{"schema_version":2,"layout":{}}"#,
        r#"{"schema_version":3,"layout":{"future":"shape"}}"#,
        r#"{"schema_version":0,"layout":{}}"#,
    ] {
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
        assert!(!root.join(V1_BACKUP_FILE_NAME).exists());
        assert!(
            !root.join("runtime").exists(),
            "no directories for {content}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_on_a_v1_drive_succeeds_and_offers_migration_without_mutation() {
    let root = temp_root("status-v1");
    let manifest_path = root.join(MANIFEST_FILE_NAME);
    fs::write(&manifest_path, V1_MANIFEST).unwrap();

    let status = status_at(&root, true, Some(20)).unwrap();
    assert_eq!(status.schema_version, Some(1));
    assert!(status.migration_required());
    let advice = status.migration_advice().unwrap();
    assert!(advice.contains("aidex init --drive"), "{advice}");
    assert_eq!(fs::read_to_string(&manifest_path).unwrap(), V1_MANIFEST);
    assert!(!root.join(V1_BACKUP_FILE_NAME).exists());

    initialize_at(&root).unwrap();
    let status = status_at(&root, true, Some(20)).unwrap();
    assert_eq!(status.schema_version, Some(CURRENT_SCHEMA_VERSION));
    assert!(!status.migration_required());
    assert_eq!(status.migration_advice(), None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn writes_only_the_canonical_root_manifest() {
    let root = temp_root("canonical");
    initialize_at(&root).unwrap();

    assert!(root.join(MANIFEST_FILE_NAME).is_file());
    assert!(!root.join(".aidex").exists());
    assert!(!root.join(".aidex/manifest.toml").exists());
    assert!(!root.join(V1_BACKUP_FILE_NAME).exists());
    assert!(
        DriveManifest::from_json(&fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap())
            .is_ok()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn init_options_default_to_requiring_removable_media() {
    assert!(InitOptions::default().require_removable);

    let root = tempfile::tempdir().unwrap();
    assert!(
        init(root.path()).is_err(),
        "a temporary directory is not a removable drive root"
    );
    assert!(!root.path().join(MANIFEST_FILE_NAME).exists());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn init_with_developer_folder_initializes_a_v2_drive_that_services_can_open() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(MANIFEST_FILE_NAME), V1_MANIFEST).unwrap();
    let options = InitOptions {
        require_removable: false,
    };

    let status = init_with(root.path(), options).unwrap();
    assert_eq!(status.schema_version, Some(CURRENT_SCHEMA_VERSION));
    assert!(status.writable);
    assert!(status.free_bytes.is_some_and(|free| free >= MIN_FREE_BYTES));
    assert_eq!(status.root, fs::canonicalize(root.path()).unwrap());
    assert_eq!(
        fs::read_to_string(root.path().join(V1_BACKUP_FILE_NAME)).unwrap(),
        V1_MANIFEST
    );

    let paths = DrivePaths::open(root.path()).unwrap();
    for (name, relative) in paths.layout().entries() {
        assert!(
            relative.resolve_under(paths.root()).is_dir(),
            "missing {name}"
        );
    }
    let again = init_with(root.path(), options).unwrap();
    assert_eq!(again.schema_version, Some(CURRENT_SCHEMA_VERSION));
    assert_eq!(
        fs::read_to_string(root.path().join(V1_BACKUP_FILE_NAME)).unwrap(),
        V1_MANIFEST
    );
}
