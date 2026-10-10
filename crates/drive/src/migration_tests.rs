use std::{fs, path::Path};

use super::{MIGRATION_TEMP_FILE_NAME, V1_BACKUP_FILE_NAME};
use crate::{
    CURRENT_SCHEMA_VERSION, DriveError, DriveManifest, MANIFEST_FILE_NAME, StoredManifest,
    ops::initialize_at,
};

/// A v1 manifest with a custom runtime path and non-canonical formatting, so the
/// backup must preserve exact bytes rather than a re-encoding.
const V1_MANIFEST: &str = "{\n  \"schema_version\": 1,\n  \"layout\": {\n    \"public_runtime\": \"bin/runtime\",\n    \"public_models\": \"models\",\n    \"private_vault\": \"private/vault\",\n    \"private_identity\": \"private/identity\",\n    \"private_sessions\": \"private/sessions\",\n    \"private_config\": \"private/config\"\n  }\n}\n";

fn v1_drive() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(MANIFEST_FILE_NAME), V1_MANIFEST).unwrap();
    root
}

fn installed(root: &Path) -> StoredManifest {
    StoredManifest::from_json(&fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap()).unwrap()
}

fn assert_untouched_v1(root: &Path) {
    assert_eq!(
        fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap(),
        V1_MANIFEST
    );
    assert!(!root.join(MIGRATION_TEMP_FILE_NAME).exists());
    for created_by_v2 in ["agents", "cache", "workspaces", "private"] {
        assert!(
            !root.join(created_by_v2).exists(),
            "{created_by_v2} was created by an aborted migration"
        );
    }
}

#[test]
fn v1_migration_backs_up_exact_bytes_and_preserves_layout_and_user_files() {
    let drive = v1_drive();
    let root = drive.path();
    fs::write(root.join("notes.txt"), "user notes").unwrap();
    fs::create_dir_all(root.join("bin/runtime")).unwrap();
    fs::write(root.join("bin/runtime/llama.bin"), b"weights").unwrap();

    initialize_at(root).unwrap();

    assert_eq!(
        fs::read(root.join(V1_BACKUP_FILE_NAME)).unwrap(),
        V1_MANIFEST.as_bytes()
    );
    let StoredManifest::V2(manifest) = installed(root) else {
        panic!("manifest was not migrated to v2");
    };
    assert_eq!(manifest.schema_version(), CURRENT_SCHEMA_VERSION);
    assert_eq!(manifest.layout.public_runtime.as_str(), "bin/runtime");
    assert_eq!(
        manifest.layout.private_sessions.as_str(),
        "private/sessions"
    );
    for (name, relative) in manifest.layout.entries() {
        assert!(relative.resolve_under(root).is_dir(), "missing {name}");
    }
    assert!(
        !root.join("runtime").exists(),
        "v1 custom path must not revert"
    );
    assert_eq!(
        fs::read_to_string(root.join("notes.txt")).unwrap(),
        "user notes"
    );
    assert_eq!(
        fs::read(root.join("bin/runtime/llama.bin")).unwrap(),
        b"weights"
    );
    assert!(!root.join(MIGRATION_TEMP_FILE_NAME).exists());

    let migrated_bytes = fs::read(root.join(MANIFEST_FILE_NAME)).unwrap();
    initialize_at(root).unwrap();
    assert_eq!(
        fs::read(root.join(MANIFEST_FILE_NAME)).unwrap(),
        migrated_bytes,
        "init is idempotent on v2"
    );
    assert_eq!(
        fs::read(root.join(V1_BACKUP_FILE_NAME)).unwrap(),
        V1_MANIFEST.as_bytes()
    );
}

#[test]
fn migration_retries_after_interruption_at_each_step() {
    // Interrupted after the backup was written; the manifest is still v1.
    let drive = v1_drive();
    let root = drive.path();
    fs::write(root.join(V1_BACKUP_FILE_NAME), V1_MANIFEST).unwrap();
    initialize_at(root).unwrap();
    assert!(matches!(installed(root), StoredManifest::V2(_)));
    assert_eq!(
        fs::read_to_string(root.join(V1_BACKUP_FILE_NAME)).unwrap(),
        V1_MANIFEST
    );

    // Interrupted after staging a (possibly partial) v2 file, before the rename.
    let drive = v1_drive();
    let root = drive.path();
    fs::write(root.join(V1_BACKUP_FILE_NAME), V1_MANIFEST).unwrap();
    fs::write(root.join(MIGRATION_TEMP_FILE_NAME), "{\"schema_vers").unwrap();
    initialize_at(root).unwrap();
    assert!(matches!(installed(root), StoredManifest::V2(_)));
    assert!(!root.join(MIGRATION_TEMP_FILE_NAME).exists());

    // Interrupted after the rename, before directories were created.
    let drive = tempfile::tempdir().unwrap();
    let root = drive.path();
    fs::write(root.join(V1_BACKUP_FILE_NAME), V1_MANIFEST).unwrap();
    let mut layout = DriveManifest::current().layout;
    layout.public_runtime = crate::RelativePath::new("bin/runtime").unwrap();
    fs::write(
        root.join(MANIFEST_FILE_NAME),
        DriveManifest::new(layout).to_json().unwrap(),
    )
    .unwrap();
    initialize_at(root).unwrap();
    assert!(root.join("bin/runtime").is_dir());
    assert!(root.join("private/index").is_dir());
}

#[test]
fn conflicting_backup_aborts_without_changing_anything() {
    let drive = v1_drive();
    let root = drive.path();
    fs::write(root.join(V1_BACKUP_FILE_NAME), "someone else's file").unwrap();

    let error = initialize_at(root).unwrap_err();
    assert!(
        matches!(&error, DriveError::MigrationConflict(path) if path.ends_with(V1_BACKUP_FILE_NAME)),
        "{error:?}"
    );
    assert!(error.to_string().contains("nothing was changed"));
    assert_eq!(
        fs::read_to_string(root.join(V1_BACKUP_FILE_NAME)).unwrap(),
        "someone else's file"
    );
    assert_untouched_v1(root);

    let drive = v1_drive();
    let root = drive.path();
    fs::create_dir(root.join(V1_BACKUP_FILE_NAME)).unwrap();
    assert!(matches!(
        initialize_at(root),
        Err(DriveError::MigrationConflict(_))
    ));
    assert_untouched_v1(root);
}

#[test]
fn v1_layouts_that_cannot_become_valid_v2_are_never_rewritten() {
    let colliding = V1_MANIFEST.replace("bin/runtime", "cache");
    let drive = tempfile::tempdir().unwrap();
    let root = drive.path();
    fs::write(root.join(MANIFEST_FILE_NAME), &colliding).unwrap();

    let error = initialize_at(root).unwrap_err();
    assert!(matches!(error, DriveError::MigrationFailed(_)), "{error:?}");
    assert!(error.to_string().contains("left unchanged"));
    assert_eq!(
        fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap(),
        colliding
    );
    assert!(!root.join(V1_BACKUP_FILE_NAME).exists());
    assert!(!root.join(MIGRATION_TEMP_FILE_NAME).exists());
}

#[test]
fn malformed_and_unknown_manifests_are_never_migrated() {
    for content in [
        V1_MANIFEST.replace("\"private_config\"", "\"private_cfg\""),
        V1_MANIFEST.replace("\"schema_version\": 1", "\"schema_version\": 9"),
        V1_MANIFEST.replace("bin/runtime", "../escape"),
        "{\"schema_version\": 1".to_owned(),
    ] {
        let drive = tempfile::tempdir().unwrap();
        let root = drive.path();
        fs::write(root.join(MANIFEST_FILE_NAME), &content).unwrap();
        assert!(
            matches!(initialize_at(root), Err(DriveError::CorruptManifest(..))),
            "{content}"
        );
        assert_eq!(
            fs::read_to_string(root.join(MANIFEST_FILE_NAME)).unwrap(),
            content
        );
        assert!(!root.join(V1_BACKUP_FILE_NAME).exists());
        assert!(!root.join(MIGRATION_TEMP_FILE_NAME).exists());
    }
}
