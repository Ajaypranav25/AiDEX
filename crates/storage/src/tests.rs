use crate::{FileStore, StoreError};
use std::fs;
use tempfile::tempdir;

#[test]
fn persists_key_value_replacement_across_reopen() {
    let root = tempdir().unwrap();
    let store = FileStore::open(root.path()).unwrap();
    store.put("profiles/default", b"old").unwrap();
    store.put("profiles/default", b"committed").unwrap();
    assert_eq!(
        store.get("profiles/default").unwrap().as_deref(),
        Some(b"committed".as_slice())
    );
    assert_eq!(store.keys().unwrap(), vec!["profiles/default"]);
    drop(store);

    let reopened = FileStore::open(root.path()).unwrap();
    assert_eq!(
        reopened.get("profiles/default").unwrap().as_deref(),
        Some(b"committed".as_slice())
    );
}

#[test]
fn dropping_an_interrupted_transaction_keeps_the_previous_committed_value() {
    let root = tempdir().unwrap();
    let store = FileStore::open(root.path()).unwrap();
    store.put("profiles/default", b"committed").unwrap();

    let mut connection = rusqlite::Connection::open(root.path().join("state.sqlite3")).unwrap();
    let transaction = connection.transaction().unwrap();
    transaction
        .execute(
            "UPDATE kv SET value = ?1 WHERE key = ?2",
            rusqlite::params![b"uncommitted".as_slice(), "profiles/default"],
        )
        .unwrap();
    drop(transaction);

    assert_eq!(
        store.get("profiles/default").unwrap().as_deref(),
        Some(b"committed".as_slice())
    );
}

#[cfg(unix)]
#[test]
fn rejects_a_selected_root_that_is_a_symlink() {
    use std::os::unix::fs::symlink;

    let target = tempdir().unwrap();
    let link_parent = tempdir().unwrap();
    let selected = link_parent.path().join("selected");
    symlink(target.path(), &selected).unwrap();

    assert!(matches!(
        FileStore::open(&selected),
        Err(StoreError::InvalidKey)
    ));
    assert!(!target.path().join("state.sqlite3").exists());
}

#[cfg(unix)]
#[test]
fn accepts_a_real_root_beneath_a_symlinked_host_ancestor() {
    use std::os::unix::fs::symlink;

    // Mirrors macOS, where temporary directories live beneath `/var -> /private/var`.
    let target = tempdir().unwrap();
    let link_parent = tempdir().unwrap();
    let ancestor = link_parent.path().join("host-alias");
    symlink(target.path(), &ancestor).unwrap();

    let store = FileStore::open(ancestor.join("drive")).unwrap();
    store.put("profiles/default", b"value").unwrap();
    assert!(target.path().join("drive").join("state.sqlite3").exists());
    assert_eq!(
        store.root(),
        fs::canonicalize(target.path().join("drive")).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn rejects_traversal_and_managed_symlink_escape() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    symlink(outside.path(), root.path().join("artifacts")).unwrap();
    assert!(matches!(
        FileStore::open(root.path()),
        Err(StoreError::InvalidKey)
    ));

    let store = FileStore::open(outside.path()).unwrap();
    assert!(matches!(
        store.put("../escape", b"x"),
        Err(StoreError::InvalidKey)
    ));
}

#[test]
fn artifacts_deduplicate_and_detect_corruption() {
    let root = tempdir().unwrap();
    let store = FileStore::open(root.path()).unwrap();
    let hash = store.put_artifact(b"payload").unwrap();
    assert_eq!(store.put_artifact(b"payload").unwrap(), hash);
    assert_eq!(store.get_artifact(&hash).unwrap(), b"payload");
    fs::write(store.root().join("artifacts").join(&hash), b"tampered").unwrap();
    assert!(matches!(
        store.get_artifact(&hash),
        Err(StoreError::Corrupt(_))
    ));
}

#[test]
fn rejects_newer_schema_with_actionable_migration_error() {
    let root = tempdir().unwrap();
    let store = FileStore::open(root.path()).unwrap();
    drop(store);
    let connection = rusqlite::Connection::open(root.path().join("state.sqlite3")).unwrap();
    connection.pragma_update(None, "user_version", 99).unwrap();
    drop(connection);

    assert!(matches!(
        FileStore::open(root.path()),
        Err(StoreError::Migration(_))
    ));
}

#[test]
fn maps_storage_failures_to_actionable_categories() {
    use std::io::{Error, ErrorKind};

    assert!(matches!(
        StoreError::from(Error::from(ErrorKind::StorageFull)),
        StoreError::LowSpace(_)
    ));
    assert!(matches!(
        StoreError::from(Error::from(ErrorKind::ReadOnlyFilesystem)),
        StoreError::ReadOnly(_)
    ));
    assert!(matches!(
        StoreError::from(Error::from(ErrorKind::NotFound)),
        StoreError::DriveUnavailable(_)
    ));
}

#[test]
fn reports_corrupt_database_instead_of_succeeding() {
    let root = tempdir().unwrap();
    let store = FileStore::open(root.path()).unwrap();
    drop(store);
    fs::write(root.path().join("state.sqlite3"), b"not a sqlite database").unwrap();

    assert!(matches!(
        FileStore::open(root.path()),
        Err(StoreError::Corrupt(_))
    ));
}

#[test]
fn removed_drive_during_later_write_reports_unavailable() {
    let root = tempdir().unwrap();
    let store = FileStore::open(root.path()).unwrap();
    fs::remove_dir_all(root.path()).unwrap();

    assert!(matches!(
        store.put("key", b"value"),
        Err(StoreError::DriveUnavailable(_))
    ));
}

#[test]
fn imports_legacy_file_values_once_without_overwriting_newer_state() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("profiles")).unwrap();
    fs::write(root.path().join("profiles/default"), b"legacy").unwrap();

    let store = FileStore::open(root.path()).unwrap();
    assert_eq!(
        store.get("profiles/default").unwrap().as_deref(),
        Some(b"legacy".as_slice())
    );
    store.put("profiles/default", b"new state").unwrap();
    drop(store);

    let reopened = FileStore::open(root.path()).unwrap();
    assert_eq!(
        reopened.get("profiles/default").unwrap().as_deref(),
        Some(b"new state".as_slice())
    );
}

#[test]
fn legacy_import_preserves_an_existing_committed_database_value() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("profiles")).unwrap();
    fs::write(root.path().join("profiles/default"), b"legacy").unwrap();
    let database = rusqlite::Connection::open(root.path().join("state.sqlite3")).unwrap();
    database
        .execute_batch(
            "CREATE TABLE kv (key TEXT PRIMARY KEY NOT NULL, value BLOB NOT NULL);
             CREATE TABLE artifacts (hash TEXT PRIMARY KEY NOT NULL, size INTEGER NOT NULL CHECK(size >= 0));
             CREATE TABLE store_metadata (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
             PRAGMA user_version = 2;",
        )
        .unwrap();
    database
        .execute(
            "INSERT INTO kv(key, value) VALUES (?1, ?2)",
            rusqlite::params!["profiles/default", b"committed".as_slice()],
        )
        .unwrap();
    drop(database);

    let store = FileStore::open(root.path()).unwrap();
    assert_eq!(
        store.get("profiles/default").unwrap().as_deref(),
        Some(b"committed".as_slice())
    );
}
