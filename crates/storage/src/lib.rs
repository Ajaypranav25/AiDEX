//! Crash-safe key/value and content-addressed artifact storage beneath a drive root.
//!
//! Structured state uses SQLite (`state.sqlite3`) with WAL journaling, full synchronous
//! commits, and schema migrations. This keeps metadata changes transactional. Artifacts are
//! SHA-256-addressed files under `artifacts/`; a temporary file is synced and atomically
//! renamed before its metadata transaction commits. Reads verify the content hash.
//! SQLite is used instead of another file-backed index because its embedded transactions
//! preserve committed values without a separate service. On first open, prior file-backed
//! key/value entries are imported once in a transaction; existing database values win.
//!
//! The selected root is canonicalized at initialization. Managed directories and database
//! paths are checked for symlinks, and logical keys reject absolute paths and traversal.
//! Use one store per process/root; SQLite serializes writers across connections/processes.

use std::{
    error::Error,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum StoreError {
    InvalidKey,
    Corrupt(String),
    Migration(String),
    LowSpace(String),
    ReadOnly(String),
    DriveUnavailable(String),
    Io(io::Error),
    Database(rusqlite::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey => f.write_str("storage key must be a non-empty safe relative path"),
            Self::Corrupt(message) => write!(f, "storage corruption detected: {message}"),
            Self::Migration(message) => write!(f, "storage migration failed: {message}"),
            Self::LowSpace(message) => write!(
                f,
                "storage write failed because the drive is full: {message}"
            ),
            Self::ReadOnly(message) => write!(f, "storage drive is read-only: {message}"),
            Self::DriveUnavailable(message) => write!(f, "storage drive is unavailable: {message}"),
            Self::Io(error) => write!(f, "storage I/O error: {error}"),
            Self::Database(error) => write!(f, "storage database error: {error}"),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        let message = error.to_string();
        match error.kind() {
            io::ErrorKind::StorageFull => Self::LowSpace(message),
            io::ErrorKind::ReadOnlyFilesystem => Self::ReadOnly(message),
            io::ErrorKind::NotFound => Self::DriveUnavailable(message),
            _ => Self::Io(error),
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(code, _) = &error {
            let message = error.to_string();
            match code.code {
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => {
                    return Self::Corrupt(message);
                }
                rusqlite::ErrorCode::DiskFull => return Self::LowSpace(message),
                rusqlite::ErrorCode::ReadOnly | rusqlite::ErrorCode::PermissionDenied => {
                    return Self::ReadOnly(message);
                }
                rusqlite::ErrorCode::CannotOpen | rusqlite::ErrorCode::NotFound => {
                    return Self::DriveUnavailable(message);
                }
                _ => {}
            }
        }
        Self::Database(error)
    }
}

/// Drive-relative durable state and content-addressed artifact repository.
#[derive(Clone, Debug)]
pub struct FileStore {
    root: PathBuf,
    database: PathBuf,
    artifacts: PathBuf,
}

impl FileStore {
    /// Open or initialize a store beneath `root`. Existing schemas are migrated atomically.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let requested_root = root.into();
        fs::create_dir_all(&requested_root)?;
        let root = fs::canonicalize(&requested_root)?;
        reject_symlink(&root)?;
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

    /// Persist bytes once by SHA-256 and return the lowercase hexadecimal content hash.
    pub fn put_artifact(&self, bytes: &[u8]) -> Result<String, StoreError> {
        let hash = hex_hash(bytes);
        let destination = self.artifacts.join(&hash);
        if destination.exists() {
            verify_blob(&destination, &hash)?;
        } else {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary = self
                .artifacts
                .join(format!(".tmp-{}-{sequence}", std::process::id()));
            let result = write_atomic_blob(&temporary, &destination, bytes);
            if let Err(error) = result {
                let _ = fs::remove_file(&temporary);
                if !destination.exists() {
                    return Err(error);
                }
                verify_blob(&destination, &hash)?;
            }
        }
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR IGNORE INTO artifacts(hash, size) VALUES (?1, ?2)",
            params![hash, bytes.len() as i64],
        )?;
        Ok(hash)
    }

    /// Read and verify an artifact. Missing metadata or content is reported as corruption.
    pub fn get_artifact(&self, hash: &str) -> Result<Vec<u8>, StoreError> {
        if !valid_hash(hash) {
            return Err(StoreError::InvalidKey);
        }
        let connection = self.connection()?;
        let size: Option<i64> = connection
            .query_row(
                "SELECT size FROM artifacts WHERE hash = ?1",
                [hash],
                |row| row.get(0),
            )
            .optional()?;
        if size.is_none() {
            return Err(StoreError::Corrupt(format!(
                "artifact metadata missing for {hash}"
            )));
        }
        let path = self.artifacts.join(hash);
        reject_symlink_if_exists(&path)?;
        let bytes = fs::read(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                StoreError::Corrupt(format!("artifact file missing for {hash}"))
            } else {
                StoreError::Io(error)
            }
        })?;
        if bytes.len() as i64 != size.unwrap_or_default() || hex_hash(&bytes) != hash {
            return Err(StoreError::Corrupt(format!(
                "artifact hash/size mismatch for {hash}"
            )));
        }
        Ok(bytes)
    }

    /// Return the canonical selected root; all managed paths are relative to it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn connection(&self) -> Result<Connection, StoreError> {
        reject_symlink_if_exists(&self.database)?;
        let connection = Connection::open(&self.database)?;
        connection.busy_timeout(std::time::Duration::from_secs(10))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(connection)
    }
}

fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let mut version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > 2 {
        return Err(StoreError::Migration(format!(
            "database schema version {version} is newer than supported version 2"
        )));
    }
    if version == 0 {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|e| StoreError::Migration(e.to_string()))?;
        transaction
            .execute_batch(
                "CREATE TABLE kv (key TEXT PRIMARY KEY NOT NULL, value BLOB NOT NULL);
                 CREATE TABLE artifacts (hash TEXT PRIMARY KEY NOT NULL, size INTEGER NOT NULL CHECK(size >= 0));
                 PRAGMA user_version = 1;",
            )
            .map_err(|e| StoreError::Migration(e.to_string()))?;
        transaction
            .commit()
            .map_err(|e| StoreError::Migration(e.to_string()))?;
        version = 1;
    }
    if version == 1 {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|e| StoreError::Migration(e.to_string()))?;
        transaction
            .execute_batch(
                "CREATE TABLE store_metadata (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
                 PRAGMA user_version = 2;",
            )
            .map_err(|e| StoreError::Migration(e.to_string()))?;
        transaction
            .commit()
            .map_err(|e| StoreError::Migration(e.to_string()))?;
    }
    Ok(())
}

fn import_legacy_files(root: &Path, connection: &Connection) -> Result<(), StoreError> {
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
        .map_err(|e| StoreError::Migration(e.to_string()))?;
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
        .map_err(|e| StoreError::Migration(e.to_string()))?;
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

fn validate_key(key: &str) -> Result<(), StoreError> {
    if key.is_empty()
        || key.contains('\\')
        || key.contains(':')
        || Path::new(key)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(StoreError::InvalidKey);
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<(), StoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(StoreError::InvalidKey);
    }
    Ok(())
}

fn reject_symlink_if_exists(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreError::InvalidKey),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn create_managed_dir(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(StoreError::InvalidKey)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn write_atomic_blob(temporary: &Path, destination: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, destination)?;
    Ok(())
}

fn verify_blob(path: &Path, expected_hash: &str) -> Result<(), StoreError> {
    reject_symlink_if_exists(path)?;
    let bytes = fs::read(path)?;
    if hex_hash(&bytes) != expected_hash {
        return Err(StoreError::Corrupt(format!(
            "artifact content mismatch for {expected_hash}"
        )));
    }
    Ok(())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn hex_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{FileStore, StoreError};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn key_value_replacement_and_migration_survive_reopen() {
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
    fn storage_failures_have_actionable_categories() {
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
    fn migrates_legacy_file_values_once_without_overwriting_newer_state() {
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
}
