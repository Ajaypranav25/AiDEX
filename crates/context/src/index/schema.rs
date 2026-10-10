//! Index database schema, connection setup, integrity checks, and recovery.
//!
//! The index is derived data: an incompatible or corrupt database is never
//! repaired in place. A corrupt file is moved aside as
//! `index.sqlite3.corrupt-<unix-ms>` (the newest [`KEEP_CORRUPT`] are kept for
//! inspection) and an empty index is created, so the next build rebuilds it.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use aidex_core::Timestamp;
use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::ContextError;

/// Database file inside `DrivePaths::index()/<workspace-id>/`.
pub const INDEX_FILE: &str = "index.sqlite3";
/// `PRAGMA user_version` of the current schema.
pub const SCHEMA_VERSION: i64 = 1;
/// Corrupt databases kept beside the index.
const KEEP_CORRUPT: usize = 3;
const CORRUPT_MARKER: &str = ".corrupt-";

const SCHEMA: &str = r#"
CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    size INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    language TEXT,
    lines INTEGER NOT NULL,
    redacted INTEGER NOT NULL DEFAULT 0,
    indexed_at INTEGER NOT NULL
);
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id),
    ordinal INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    text TEXT NOT NULL,
    UNIQUE (file_id, ordinal)
);
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    text,
    content = 'chunks',
    content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts (rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts (chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TABLE symbols (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id),
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    line INTEGER NOT NULL
);
CREATE INDEX symbols_name ON symbols (name COLLATE NOCASE);
CREATE INDEX symbols_file ON symbols (file_id);
CREATE TABLE vectors (
    chunk_id INTEGER NOT NULL REFERENCES chunks(id),
    model TEXT NOT NULL,
    dims INTEGER NOT NULL,
    vector BLOB NOT NULL,
    PRIMARY KEY (chunk_id, model)
);
"#;

/// Why an index was discarded and rebuilt from scratch when it was opened.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexRecovery {
    pub kind: RecoveryKind,
    pub reason: String,
    /// File name (inside the index directory) the corrupt database was moved to.
    pub moved_to: Option<String>,
    pub at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind {
    /// The database failed to open or failed its integrity check.
    Corrupt,
    /// The database had another schema version; it was deleted and recreated.
    SchemaReset,
}

enum OpenFailure {
    Corrupt(String),
    Schema(String),
    Other(ContextError),
}

fn is_corruption(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt)
    )
}

fn classify(context: &'static str) -> impl Fn(rusqlite::Error) -> OpenFailure {
    move |error| {
        if is_corruption(&error) {
            OpenFailure::Corrupt(format!("{context}: {error}"))
        } else {
            OpenFailure::Other(ContextError::sqlite(context, error))
        }
    }
}

/// Apply the connection settings every index connection uses.
pub(crate) fn configure(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.busy_timeout(Duration::from_secs(10))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// Open another connection to an index that [`open`] already validated.
pub(crate) fn connect(path: &Path) -> Result<Connection, ContextError> {
    refuse_link(path)?;
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|error| ContextError::sqlite("cannot open the index", error))?;
    configure(&connection).map_err(|error| ContextError::sqlite("cannot open the index", error))?;
    Ok(connection)
}

fn refuse_link(path: &Path) -> Result<(), ContextError> {
    for candidate in [
        path.to_path_buf(),
        sidecar(path, "-wal"),
        sidecar(path, "-shm"),
    ] {
        if fs::symlink_metadata(&candidate).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(ContextError::Index(format!(
                "refusing symlinked index file `{}`",
                candidate.display()
            )));
        }
    }
    Ok(())
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Open (creating if missing) and validate the index database in `dir`.
///
/// Returns the connection and, when the existing database had to be discarded, why.
pub(crate) fn open(dir: &Path) -> Result<(Connection, Option<IndexRecovery>), ContextError> {
    let path = dir.join(INDEX_FILE);
    refuse_link(&path)?;
    let recovery = match try_open(&path) {
        Ok(connection) => return Ok((connection, None)),
        Err(OpenFailure::Other(error)) => return Err(error),
        Err(OpenFailure::Corrupt(reason)) => {
            let moved_to = move_aside(dir, &path)?;
            IndexRecovery {
                kind: RecoveryKind::Corrupt,
                reason,
                moved_to: Some(moved_to),
                at: Timestamp::now(),
            }
        }
        Err(OpenFailure::Schema(reason)) => {
            for file in [path.clone(), sidecar(&path, "-wal"), sidecar(&path, "-shm")] {
                remove_if_present(&file)?;
            }
            IndexRecovery {
                kind: RecoveryKind::SchemaReset,
                reason,
                moved_to: None,
                at: Timestamp::now(),
            }
        }
    };
    match try_open(&path) {
        Ok(connection) => Ok((connection, Some(recovery))),
        Err(OpenFailure::Other(error)) => Err(error),
        Err(OpenFailure::Corrupt(reason) | OpenFailure::Schema(reason)) => Err(
            ContextError::Index(format!("a fresh index could not be created: {reason}")),
        ),
    }
}

fn try_open(path: &Path) -> Result<Connection, OpenFailure> {
    let connection = Connection::open(path).map_err(classify("cannot open the index"))?;
    configure(&connection).map_err(classify("cannot configure the index"))?;
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(classify("cannot read the index schema version"))?;
    let tables: i64 = connection
        .query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))
        .map_err(classify("cannot read the index schema"))?;
    if version == 0 && tables == 0 {
        connection
            .execute_batch(&format!(
                "BEGIN IMMEDIATE; {SCHEMA} PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;"
            ))
            .map_err(classify("cannot create the index schema"))?;
        return Ok(connection);
    }
    if version != SCHEMA_VERSION {
        return Err(OpenFailure::Schema(format!(
            "index schema version {version} is not {SCHEMA_VERSION}"
        )));
    }
    let check: String = connection
        .query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))
        .map_err(classify("index integrity check failed"))?;
    if check != "ok" {
        return Err(OpenFailure::Corrupt(format!(
            "index integrity check failed: {check}"
        )));
    }
    connection
        .execute(
            "INSERT INTO chunks_fts (chunks_fts, rank) VALUES ('integrity-check', 1)",
            [],
        )
        .map_err(|error| match error.sqlite_error_code() {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => {
                OpenFailure::Other(ContextError::sqlite("full-text index check failed", error))
            }
            _ => OpenFailure::Corrupt(format!("full-text index check failed: {error}")),
        })?;
    Ok(connection)
}

/// Move a corrupt database (and its WAL) aside; drop its shared-memory file.
fn move_aside(dir: &Path, path: &Path) -> Result<String, ContextError> {
    let name = format!(
        "{INDEX_FILE}{CORRUPT_MARKER}{}",
        Timestamp::now().unix_millis()
    );
    let target = dir.join(&name);
    fs::rename(path, &target)
        .map_err(|error| ContextError::io("cannot move the corrupt index aside", error))?;
    let wal = sidecar(path, "-wal");
    if wal.exists() {
        fs::rename(&wal, sidecar(&target, "-wal"))
            .map_err(|error| ContextError::io("cannot move the corrupt index log aside", error))?;
    }
    remove_if_present(&sidecar(path, "-shm"))?;
    prune_corrupt(dir);
    Ok(name)
}

/// Keep only the newest [`KEEP_CORRUPT`] corrupt copies (best effort).
fn prune_corrupt(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("{INDEX_FILE}{CORRUPT_MARKER}");
    let mut stamps: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| {
            let stamp = name.strip_prefix(&prefix)?;
            let stamp = stamp.strip_suffix("-wal").unwrap_or(stamp);
            stamp
                .bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| stamp.to_owned())
        })
        .collect();
    stamps.sort_by_key(|stamp| stamp.parse::<u64>().unwrap_or(0));
    stamps.dedup();
    let excess = stamps.len().saturating_sub(KEEP_CORRUPT);
    for stamp in &stamps[..excess] {
        for suffix in ["", "-wal"] {
            let file = dir.join(format!("{prefix}{stamp}{suffix}"));
            if fs::symlink_metadata(&file).is_ok_and(|metadata| metadata.is_file()) {
                let _ = fs::remove_file(file);
            }
        }
    }
}

fn remove_if_present(path: &Path) -> Result<(), ContextError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ContextError::io(
            format!("cannot remove `{}`", path.display()),
            error,
        )),
    }
}

/// Read a `meta` value.
pub(crate) fn meta_get(connection: &Connection, key: &str) -> Result<Option<String>, ContextError> {
    connection
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|error| ContextError::sqlite("cannot read index metadata", error))
}

/// Write a `meta` value.
pub(crate) fn meta_set(
    connection: &Connection,
    key: &str,
    value: &str,
) -> Result<(), ContextError> {
    connection
        .execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map(|_| ())
        .map_err(|error| ContextError::sqlite("cannot write index metadata", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_databases_get_the_current_schema() {
        let dir = tempfile::tempdir().unwrap();
        let (connection, recovery) = open(dir.path()).unwrap();
        assert!(recovery.is_none());
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        meta_set(&connection, "k", "v").unwrap();
        meta_set(&connection, "k", "w").unwrap();
        assert_eq!(meta_get(&connection, "k").unwrap().as_deref(), Some("w"));
        drop(connection);
        let (_connection, recovery) = open(dir.path()).unwrap();
        assert!(recovery.is_none());
    }

    #[test]
    fn other_schema_versions_are_reset() {
        let dir = tempfile::tempdir().unwrap();
        {
            let connection = Connection::open(dir.path().join(INDEX_FILE)).unwrap();
            connection
                .execute_batch("CREATE TABLE old (x); PRAGMA user_version = 99;")
                .unwrap();
        }
        let (_connection, recovery) = open(dir.path()).unwrap();
        let recovery = recovery.unwrap();
        assert_eq!(recovery.kind, RecoveryKind::SchemaReset);
        assert!(recovery.moved_to.is_none());
    }

    #[test]
    fn only_the_newest_corrupt_copies_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        for stamp in 1..=5 {
            fs::write(
                dir.path()
                    .join(format!("{INDEX_FILE}{CORRUPT_MARKER}{stamp}")),
                b"x",
            )
            .unwrap();
        }
        fs::write(dir.path().join("unrelated"), b"x").unwrap();
        prune_corrupt(dir.path());
        let mut names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                format!("{INDEX_FILE}{CORRUPT_MARKER}3"),
                format!("{INDEX_FILE}{CORRUPT_MARKER}4"),
                format!("{INDEX_FILE}{CORRUPT_MARKER}5"),
                "unrelated".to_owned(),
            ]
        );
    }
}
