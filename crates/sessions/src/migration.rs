//! Schema of `sessions.sqlite3`, versioned in `PRAGMA user_version`.
//!
//! Migrations run in one immediate transaction each, in order; a database written by
//! a newer build is refused untouched. The process-local `SessionCatalog` that this
//! crate replaced never persisted anything, so there is no earlier on-disk format.

use rusqlite::Connection;

use crate::SessionError;

/// Current schema version.
pub const SCHEMA_VERSION: i32 = 1;

/// `MIGRATIONS[n]` upgrades schema `n` to `n + 1`.
const MIGRATIONS: &[&str] = &[V1];

const V1: &str = "
CREATE TABLE sessions (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    title TEXT NOT NULL,
    state TEXT NOT NULL,
    state_json TEXT NOT NULL,
    parent_session_id TEXT REFERENCES sessions(id),
    branched_from TEXT,
    model_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    host_json TEXT NOT NULL,
    last_event_cursor INTEGER NOT NULL DEFAULT 0 CHECK (last_event_cursor >= 0),
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    tool_call_count INTEGER NOT NULL DEFAULT 0,
    wall_ms INTEGER NOT NULL DEFAULT 0,
    next_message_seq INTEGER NOT NULL DEFAULT 1,
    next_checkpoint_seq INTEGER NOT NULL DEFAULT 1,
    next_tool_ordinal INTEGER NOT NULL DEFAULT 1,
    resumed_at TEXT
);
CREATE INDEX sessions_by_workspace ON sessions(workspace_id);
CREATE INDEX sessions_by_state ON sessions(state);

CREATE TABLE messages (
    session_id TEXT NOT NULL REFERENCES sessions(id),
    seq INTEGER NOT NULL CHECK (seq > 0),
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    tool_call_id TEXT,
    redacted INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (session_id, seq)
) WITHOUT ROWID;

CREATE TABLE tool_calls (
    session_id TEXT NOT NULL REFERENCES sessions(id),
    call_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    tool_id TEXT NOT NULL,
    input_json TEXT NOT NULL,
    state TEXT NOT NULL,
    result_summary TEXT,
    output_artifact_id TEXT REFERENCES artifacts(id),
    approval_id TEXT,
    process_id TEXT,
    redacted INTEGER NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    PRIMARY KEY (session_id, call_id)
);
CREATE INDEX tool_calls_by_state ON tool_calls(state);

CREATE TABLE approvals (
    session_id TEXT NOT NULL REFERENCES sessions(id),
    approval_id TEXT NOT NULL,
    intent_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL,
    scope TEXT NOT NULL,
    expires_at TEXT,
    recorded_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (session_id, approval_id)
);

CREATE TABLE artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    kind TEXT NOT NULL,
    producer TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    size INTEGER NOT NULL,
    source_paths_json TEXT NOT NULL,
    redacted INTEGER NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX artifacts_by_session ON artifacts(session_id);

CREATE TABLE checkpoints (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    seq INTEGER NOT NULL,
    label TEXT,
    sha256 TEXT NOT NULL,
    size INTEGER NOT NULL,
    format INTEGER NOT NULL,
    message_seq INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('valid', 'corrupt')),
    corrupt_reason TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (session_id, seq)
);

CREATE TABLE event_gaps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    kind TEXT NOT NULL,
    event_name TEXT,
    detail TEXT NOT NULL,
    session_cursor INTEGER,
    log_cursor INTEGER,
    detected_at TEXT NOT NULL
);
CREATE INDEX event_gaps_by_session ON event_gaps(session_id);

CREATE TABLE pending_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    event_name TEXT NOT NULL,
    created_at TEXT NOT NULL
);
";

pub(crate) fn migrate(connection: &Connection) -> Result<(), SessionError> {
    let version: i32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if !(0..=SCHEMA_VERSION).contains(&version) {
        return Err(SessionError::Incompatible(format!(
            "the session database uses schema v{version}; this build supports v{SCHEMA_VERSION} (upgrade AiDEX)"
        )));
    }
    if version == 0 && has_foreign_tables(connection)? {
        return Err(SessionError::Incompatible(
            "the session database file contains unknown tables; refusing to modify it".into(),
        ));
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let next = index + 1;
        connection.execute_batch(&format!(
            "BEGIN IMMEDIATE; {migration} PRAGMA user_version = {next}; COMMIT;"
        ))?;
    }
    Ok(())
}

fn has_foreign_tables(connection: &Connection) -> Result<bool, SessionError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}
