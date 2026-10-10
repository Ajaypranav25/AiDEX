//! SQLite persistence for the workspace registry (`private_config/workspaces.sqlite3`).

use std::{path::Path, time::Duration};

use aidex_core::{EntityId, Timestamp};
use aidex_drive::RelativePath;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{WorkspaceError, WorkspaceKind, WorkspaceRoot};

/// Registry schema version stored in `PRAGMA user_version`.
pub(crate) const SCHEMA_VERSION: i32 = 1;

const SCHEMA: &str = "
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('host', 'drive', 'ephemeral')),
    source_id TEXT REFERENCES workspaces(id),
    root_type TEXT NOT NULL CHECK (root_type IN ('host', 'drive')),
    root TEXT NOT NULL,
    root_key TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    last_opened_at TEXT,
    CHECK ((kind = 'ephemeral') = (source_id IS NOT NULL))
);
CREATE TABLE selection (
    slot INTEGER PRIMARY KEY NOT NULL CHECK (slot = 1),
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE
);
";

const COLUMNS: &str = "id, name, kind, source_id, root_type, root, created_at, last_opened_at";

/// A persisted registry row.
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub id: EntityId,
    pub name: String,
    pub kind: WorkspaceKind,
    pub root: WorkspaceRoot,
    pub created_at: Timestamp,
    pub last_opened_at: Option<Timestamp>,
}

pub(crate) fn open(path: &Path) -> Result<Connection, WorkspaceError> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(10))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&connection)?;
    Ok(connection)
}

fn migrate(connection: &Connection) -> Result<(), WorkspaceError> {
    let version: i32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        0 => {
            connection.execute_batch(&format!(
                "BEGIN IMMEDIATE; {SCHEMA} PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;"
            ))?;
            Ok(())
        }
        SCHEMA_VERSION => Ok(()),
        newer => Err(WorkspaceError::Incompatible(format!(
            "the workspace registry uses schema v{newer}; this build supports v{SCHEMA_VERSION} (upgrade AiDEX)"
        ))),
    }
}

fn decode(row: &Row<'_>) -> rusqlite::Result<Result<Record, WorkspaceError>> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let source: Option<String> = row.get(3)?;
    let root_type: String = row.get(4)?;
    let root: String = row.get(5)?;
    let created_at: String = row.get(6)?;
    let last_opened_at: Option<String> = row.get(7)?;
    Ok((|| {
        let corrupt = |what: &str| WorkspaceError::Corrupt(format!("registry row `{id}`: {what}"));
        let kind = match (kind.as_str(), source) {
            ("host", None) => WorkspaceKind::Host,
            ("drive", None) => WorkspaceKind::Drive,
            ("ephemeral", Some(source)) => WorkspaceKind::Ephemeral {
                source: EntityId::new(source).map_err(|_| corrupt("invalid source id"))?,
            },
            _ => return Err(corrupt("invalid kind")),
        };
        let root = match root_type.as_str() {
            "host" => WorkspaceRoot::Host(root),
            "drive" => WorkspaceRoot::Drive(
                RelativePath::new(&root).map_err(|_| corrupt("invalid drive root"))?,
            ),
            _ => return Err(corrupt("invalid root type")),
        };
        Ok(Record {
            id: EntityId::new(id.clone()).map_err(|_| corrupt("invalid id"))?,
            name,
            kind,
            root,
            created_at: Timestamp::new(created_at).map_err(|_| corrupt("invalid created_at"))?,
            last_opened_at: last_opened_at
                .map(Timestamp::new)
                .transpose()
                .map_err(|_| corrupt("invalid last_opened_at"))?,
        })
    })())
}

fn kind_columns(kind: &WorkspaceKind) -> (&'static str, Option<&str>) {
    match kind {
        WorkspaceKind::Host => ("host", None),
        WorkspaceKind::Drive => ("drive", None),
        WorkspaceKind::Ephemeral { source } => ("ephemeral", Some(source.as_str())),
    }
}

fn root_type(root: &WorkspaceRoot) -> &'static str {
    match root {
        WorkspaceRoot::Host(_) => "host",
        WorkspaceRoot::Drive(_) => "drive",
    }
}

/// Insert a record, reporting duplicate names and roots as conflicts.
pub(crate) fn insert(
    connection: &mut Connection,
    record: &Record,
    name_key: &str,
    root_key: &str,
) -> Result<(), WorkspaceError> {
    let transaction = connection.transaction()?;
    if let Some(existing) = transaction
        .query_row(
            "SELECT id FROM workspaces WHERE name_key = ?1",
            [name_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        return Err(WorkspaceError::Conflict(format!(
            "a workspace named `{}` is already registered ({existing}); choose another name",
            record.name
        )));
    }
    if let Some(existing) = transaction
        .query_row(
            "SELECT id FROM workspaces WHERE root_key = ?1",
            [root_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        return Err(WorkspaceError::Conflict(format!(
            "`{}` is already registered as workspace {existing}",
            record.root.as_str()
        )));
    }
    let (kind, source) = kind_columns(&record.kind);
    transaction.execute(
        "INSERT INTO workspaces (id, name, name_key, kind, source_id, root_type, root, root_key, created_at, last_opened_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            record.id.as_str(),
            record.name,
            name_key,
            kind,
            source,
            root_type(&record.root),
            record.root.as_str(),
            root_key,
            record.created_at.as_str(),
            record.last_opened_at.as_ref().map(Timestamp::as_str),
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

pub(crate) fn get(
    connection: &Connection,
    id: &EntityId,
) -> Result<Option<Record>, WorkspaceError> {
    connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM workspaces WHERE id = ?1"),
            [id.as_str()],
            decode,
        )
        .optional()?
        .transpose()
}

pub(crate) fn by_name(
    connection: &Connection,
    name_key: &str,
) -> Result<Option<Record>, WorkspaceError> {
    connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM workspaces WHERE name_key = ?1"),
            [name_key],
            decode,
        )
        .optional()?
        .transpose()
}

pub(crate) fn list(connection: &Connection) -> Result<Vec<Record>, WorkspaceError> {
    let mut statement = connection.prepare(&format!(
        "SELECT {COLUMNS} FROM workspaces ORDER BY created_at, rowid"
    ))?;
    let rows = statement.query_map([], decode)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row??);
    }
    Ok(records)
}

/// Ids of ephemeral workspaces copied from `source`.
pub(crate) fn children(
    connection: &Connection,
    source: &EntityId,
) -> Result<Vec<String>, WorkspaceError> {
    let mut statement =
        connection.prepare("SELECT id FROM workspaces WHERE source_id = ?1 ORDER BY id")?;
    let rows = statement.query_map([source.as_str()], |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<String>, _>>()?)
}

pub(crate) fn delete(connection: &Connection, id: &EntityId) -> Result<bool, WorkspaceError> {
    Ok(connection.execute("DELETE FROM workspaces WHERE id = ?1", [id.as_str()])? != 0)
}

pub(crate) fn touch(
    connection: &Connection,
    id: &EntityId,
    at: &Timestamp,
) -> Result<bool, WorkspaceError> {
    Ok(connection.execute(
        "UPDATE workspaces SET last_opened_at = ?2 WHERE id = ?1",
        params![id.as_str(), at.as_str()],
    )? != 0)
}

/// Persist the selection and the open time in one transaction.
pub(crate) fn select(
    connection: &mut Connection,
    id: &EntityId,
    at: &Timestamp,
) -> Result<bool, WorkspaceError> {
    let transaction = connection.transaction()?;
    if !touch(&transaction, id, at)? {
        return Ok(false);
    }
    transaction.execute(
        "INSERT INTO selection (slot, workspace_id) VALUES (1, ?1)
         ON CONFLICT(slot) DO UPDATE SET workspace_id = excluded.workspace_id",
        [id.as_str()],
    )?;
    transaction.commit()?;
    Ok(true)
}

pub(crate) fn selected(connection: &Connection) -> Result<Option<EntityId>, WorkspaceError> {
    let id: Option<String> = connection
        .query_row(
            "SELECT workspace_id FROM selection WHERE slot = 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    id.map(|id| {
        EntityId::new(id)
            .map_err(|_| WorkspaceError::Corrupt("invalid selected workspace id".into()))
    })
    .transpose()
}
