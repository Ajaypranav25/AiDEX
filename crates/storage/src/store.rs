use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    StoreError,
    migration::{import_legacy_files, migrate},
    paths::{
        create_managed_dir, reject_symlink_components, reject_symlink_if_exists, validate_key,
    },
};

/// Drive-relative durable state and content-addressed artifact repository.
#[derive(Clone, Debug)]
pub struct FileStore {
    pub(super) root: PathBuf,
    pub(super) database: PathBuf,
    pub(super) artifacts: PathBuf,
}

impl FileStore {
    /// Open or initialize a store beneath `root`. Existing schemas are migrated atomically.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let requested_root = root.into();
        reject_symlink_components(&requested_root)?;
        fs::create_dir_all(&requested_root)?;
        reject_symlink_components(&requested_root)?;
        let root = fs::canonicalize(&requested_root)?;
        let artifacts = root.join("artifacts");
        create_managed_dir(&artifacts)?;
        let database = root.join("state.sqlite3");
        reject_symlink_if_exists(&database)?;
        let store = Self {
            root,
            database,
            artifacts,
        };
        let connection = store.connection()?;
        migrate(&connection)?;
        import_legacy_files(&store.root, &connection)?;
        Ok(store)
    }

    /// Read a value, returning `None` only when no committed value exists.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        validate_key(key)?;
        let connection = self.connection()?;
        connection
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(Into::into)
    }

    /// Atomically replace a value. SQLite transactions ensure readers observe the old or new
    /// committed value; an interrupted write is never reported as a successful truncation.
    pub fn put(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        validate_key(key)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO kv(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Remove a value, returning whether a committed row existed.
    pub fn delete(&self, key: &str) -> Result<bool, StoreError> {
        validate_key(key)?;
        let connection = self.connection()?;
        Ok(connection.execute("DELETE FROM kv WHERE key = ?1", [key])? != 0)
    }

    /// List committed logical keys in sorted order.
    pub fn keys(&self) -> Result<Vec<String>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT key FROM kv ORDER BY key")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Return the canonical selected root; all managed paths are relative to it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn connection(&self) -> Result<Connection, StoreError> {
        reject_symlink_if_exists(&self.database)?;
        let connection = Connection::open(&self.database)?;
        connection.busy_timeout(std::time::Duration::from_secs(10))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(connection)
    }
}
