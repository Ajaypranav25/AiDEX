use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, params};

use crate::{StoreError, paths::validate_key};

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let mut version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > 2 {
        return Err(StoreError::Migration(format!(
            "database schema version {version} is newer than supported version 2"
        )));
    }
    if version == 0 {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|error| StoreError::Migration(error.to_string()))?;
        transaction
            .execute_batch(
                "CREATE TABLE kv (key TEXT PRIMARY KEY NOT NULL, value BLOB NOT NULL);
                 CREATE TABLE artifacts (hash TEXT PRIMARY KEY NOT NULL, size INTEGER NOT NULL CHECK(size >= 0));
                 PRAGMA user_version = 1;",
            )
            .map_err(|error| StoreError::Migration(error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| StoreError::Migration(error.to_string()))?;
        version = 1;
    }
    if version == 1 {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|error| StoreError::Migration(error.to_string()))?;
        transaction
            .execute_batch(
                "CREATE TABLE store_metadata (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
                 PRAGMA user_version = 2;",
            )
            .map_err(|error| StoreError::Migration(error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| StoreError::Migration(error.to_string()))?;
    }
    Ok(())
}

pub(super) fn import_legacy_files(root: &Path, connection: &Connection) -> Result<(), StoreError> {
    let imported: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM store_metadata WHERE key = 'legacy_file_store_imported')",
        [],
        |row| row.get(0),
    )?;
    if imported {
        return Ok(());
    }

    let mut files = Vec::new();
    collect_legacy_files(root, root, &mut files)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| StoreError::Migration(error.to_string()))?;
    for (key, path) in files {
        let value = fs::read(path)?;
        transaction.execute(
            "INSERT OR IGNORE INTO kv(key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
    }
    transaction.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('legacy_file_store_imported', 'true')",
        [],
    )?;
    transaction
        .commit()
        .map_err(|error| StoreError::Migration(error.to_string()))?;
    Ok(())
}

fn collect_legacy_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), StoreError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| StoreError::InvalidKey)?;
        if relative == Path::new("artifacts") || relative.starts_with("artifacts") {
            continue;
        }
        let relative_string = relative.to_string_lossy().replace('\\', "/");
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(StoreError::InvalidKey);
        }
        if kind.is_dir() {
            collect_legacy_files(root, &path, files)?;
        } else if kind.is_file()
            && relative_string != "state.sqlite3"
            && relative_string != "state.sqlite3-wal"
            && relative_string != "state.sqlite3-shm"
        {
            validate_key(&relative_string)?;
            files.push((relative_string, path));
        }
    }
    Ok(())
}
