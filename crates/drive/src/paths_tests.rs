use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{DrivePaths, platform_runtime_name, runtime_name_for};
use crate::{DriveError, DriveLayout, DriveManifest, MANIFEST_FILE_NAME, ops::initialize_at};

fn v2_drive() -> (tempfile::TempDir, DrivePaths) {
    let drive = tempfile::tempdir().unwrap();
    fs::write(
        drive.path().join(MANIFEST_FILE_NAME),
        DriveManifest::current().to_json().unwrap(),
    )
    .unwrap();
    let paths = DrivePaths::open(drive.path()).unwrap();
    (drive, paths)
}

#[test]
fn open_reads_a_v2_manifest_and_resolves_every_accessor() {
    let (drive, paths) = v2_drive();
    let root = fs::canonicalize(drive.path()).unwrap();
    assert_eq!(paths.root(), root);
    assert_eq!(paths.layout(), &DriveLayout::default());

    let expected: [(PathBuf, &str); 19] = [
        (paths.runtime(), "runtime"),
        (paths.models(), "models"),
        (paths.agents(), "agents"),
        (paths.skills(), "skills"),
        (paths.mcp(), "mcp"),
        (paths.plugins(), "plugins"),
        (paths.policies(), "policies"),
        (paths.workspaces(), "workspaces"),
        (paths.cache(), "cache"),
        (paths.tmp(), "tmp"),
        (paths.logs(), "logs"),
        (paths.app(), "app"),
        (paths.manifests(), "manifests"),
        (paths.vault(), "private/vault"),
        (paths.identity(), "private/identity"),
        (paths.sessions(), "private/sessions"),
        (paths.config(), "private/config"),
        (paths.memory(), "private/memory"),
        (paths.index(), "private/index"),
    ];
    for (actual, relative) in expected {
        let wanted = relative
            .split('/')
            .fold(root.clone(), |path, part| path.join(part));
        assert_eq!(actual, wanted);
        assert!(!actual.exists(), "accessors must not create directories");
    }
    assert_eq!(paths.runtime_state(), root.join(".aidex").join("runtime"));
    assert_eq!(
        paths.platform_runtime(),
        platform_runtime_name().map(|name| root.join("runtime").join(name))
    );
}

#[test]
fn open_uses_custom_layout_paths_from_the_manifest() {
    let drive = tempfile::tempdir().unwrap();
    let layout = DriveLayout {
        private_sessions: crate::RelativePath::new("state/sessions").unwrap(),
        ..DriveLayout::default()
    };
    fs::write(
        drive.path().join(MANIFEST_FILE_NAME),
        DriveManifest::new(layout.clone()).to_json().unwrap(),
    )
    .unwrap();
    let paths = DrivePaths::open(drive.path()).unwrap();
    assert!(
        paths
            .sessions()
            .ends_with(Path::new("state").join("sessions"))
    );
    assert_eq!(
        DrivePaths::from_layout(paths.root().to_path_buf(), layout),
        paths
    );
}

#[test]
fn open_requires_v2_and_never_mutates_the_drive() {
    let drive = tempfile::tempdir().unwrap();
    let manifest_path = drive.path().join(MANIFEST_FILE_NAME);
    assert!(matches!(
        DrivePaths::open(drive.path()),
        Err(DriveError::MissingManifest(_))
    ));

    let v1 = r#"{"schema_version":1,"layout":{"public_runtime":"runtime","public_models":"models","private_vault":"private/vault","private_identity":"private/identity","private_sessions":"private/sessions","private_config":"private/config"}}"#;
    fs::write(&manifest_path, v1).unwrap();
    let error = DrivePaths::open(drive.path()).unwrap_err();
    let root = fs::canonicalize(drive.path()).unwrap();
    assert!(
        matches!(&error, DriveError::MigrationRequired(path) if *path == root),
        "{error:?}"
    );
    assert_eq!(
        error.code(),
        aidex_core::ServiceErrorCode::IncompatibleVersion
    );
    assert!(error.to_string().contains("aidex init --drive"), "{error}");
    assert_eq!(fs::read_to_string(&manifest_path).unwrap(), v1);
    assert_eq!(fs::read_dir(drive.path()).unwrap().count(), 1);

    for corrupt in ["{", r#"{"schema_version":3,"layout":{}}"#] {
        fs::write(&manifest_path, corrupt).unwrap();
        assert!(matches!(
            DrivePaths::open(drive.path()),
            Err(DriveError::CorruptManifest(..))
        ));
    }

    initialize_at(&root).unwrap_err();
    fs::write(&manifest_path, v1).unwrap();
    initialize_at(&root).unwrap();
    assert!(DrivePaths::open(drive.path()).is_ok());
}

#[test]
fn ensure_dir_creates_declared_directories_and_children_only() {
    let (_drive, paths) = v2_drive();

    let sessions = paths.ensure_dir(&paths.sessions()).unwrap();
    assert_eq!(sessions, paths.sessions());
    assert!(sessions.is_dir());
    let index = paths.ensure_dir(&paths.index().join("ws_1")).unwrap();
    assert!(index.is_dir());
    assert!(paths.ensure_dir(&paths.runtime_state()).unwrap().is_dir());
    assert!(paths.ensure_dir(&paths.sessions()).is_ok(), "idempotent");
    let relative = paths.ensure_dir(Path::new("cache/./models")).unwrap();
    assert_eq!(relative, paths.cache().join("models"));
    assert!(relative.is_dir());

    let outside = tempfile::tempdir().unwrap();
    for rejected in [
        paths.root().to_path_buf(),
        paths.root().join("private"),
        paths.root().join("elsewhere"),
        paths.root().join("cachex"),
        paths.root().join(".aidex"),
        paths.cache().join("..").join("escape"),
        paths.root().parent().unwrap().join("sibling"),
        outside.path().join("cache"),
        PathBuf::from("../cache"),
        PathBuf::from(""),
    ] {
        let error = paths.ensure_dir(&rejected).unwrap_err();
        assert!(
            matches!(error, DriveError::UnmanagedPath(_)),
            "{rejected:?}: {error:?}"
        );
    }
    assert!(!paths.root().join("elsewhere").exists());
    assert!(!paths.root().join("escape").exists());
}

#[test]
fn ensure_dir_refuses_files_in_place_of_directories() {
    let (_drive, paths) = v2_drive();
    fs::write(paths.root().join("logs"), "not a directory").unwrap();
    let error = paths.ensure_dir(&paths.logs().join("daemon")).unwrap_err();
    assert!(
        matches!(&error, DriveError::UnsafeComponent(path) if path.ends_with("logs")),
        "{error:?}"
    );
    assert_eq!(error.code(), aidex_core::ServiceErrorCode::PermissionDenied);
}

#[test]
fn ensure_dir_refuses_symlinked_components() {
    let (_drive, paths) = v2_drive();
    let outside = tempfile::tempdir().unwrap();
    let link = paths.root().join("cache");
    if !make_dir_link(outside.path(), &link) {
        println!("note: skipping symlink test; this host does not permit directory links");
        return;
    }
    let error = paths.ensure_dir(&paths.cache().join("models")).unwrap_err();
    assert!(
        matches!(&error, DriveError::UnsafeComponent(path) if path.ends_with("cache")),
        "{error:?}"
    );
    assert!(!outside.path().join("models").exists());

    // Initialization refuses the same link rather than following it.
    assert!(matches!(
        initialize_at(paths.root()),
        Err(DriveError::UnsafeComponent(_))
    ));
}

#[cfg(unix)]
fn make_dir_link(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn make_dir_link(target: &Path, link: &Path) -> bool {
    if std::os::windows::fs::symlink_dir(target, link).is_ok() {
        return true;
    }
    // Junctions need no special privilege and are also refused.
    std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .output()
        .is_ok_and(|output| output.status.success())
        && fs::symlink_metadata(link).is_ok()
}

#[cfg(not(any(unix, windows)))]
fn make_dir_link(_target: &Path, _link: &Path) -> bool {
    false
}

#[test]
fn platform_runtime_names_are_normalized() {
    assert_eq!(runtime_name_for("windows", "x86_64"), Some("windows-x64"));
    assert_eq!(runtime_name_for("linux", "x86_64"), Some("linux-x64"));
    assert_eq!(runtime_name_for("linux", "aarch64"), Some("linux-arm64"));
    assert_eq!(runtime_name_for("macos", "aarch64"), Some("macos-arm64"));
    assert_eq!(runtime_name_for("macos", "x86_64"), Some("macos-x64"));
    assert_eq!(runtime_name_for("windows", "aarch64"), None);
    assert_eq!(runtime_name_for("freebsd", "x86_64"), None);
    let paths = DrivePaths::from_layout(PathBuf::from("drive"), DriveLayout::default());
    if cfg!(all(windows, target_arch = "x86_64")) {
        assert_eq!(
            paths.platform_runtime(),
            Some(Path::new("drive").join("runtime").join("windows-x64"))
        );
    }
}
