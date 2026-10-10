//! [`SessionStore`]: the durable session database on the drive.

pub(crate) mod publish;
pub(crate) mod rows;

use std::{
    fmt, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use aidex_core::{EntityId, ModelReference, Timestamp};
use aidex_drive::DrivePaths;
use aidex_events::{Cursor, EventKind, EventSink};
use aidex_storage::FileStore;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::{
    BudgetUsage, HostFingerprint, NewSession, Session, SessionError, SessionFilter, SessionState,
    SessionStateKind, migration,
    model::{MAX_REASON_BYTES, MAX_TITLE_BYTES, bounded, validate_code},
    secrets::redact,
};
use publish::{stage, transition_event};
use rows::{SESSION_COLUMNS, cursor_sql, encode, load_session, session_row, signed};

/// Session database inside `DrivePaths::sessions()`.
pub const SESSIONS_FILE: &str = "sessions.sqlite3";
/// Content-addressed blob store (checkpoints, artifacts) inside `DrivePaths::sessions()`.
pub const BLOB_DIR: &str = "store";

/// Durable sessions, messages, tool calls, approvals, artifacts, and checkpoints.
///
/// SQLite (`sessions.sqlite3`, WAL, `synchronous=FULL`) holds structured state; large
/// payloads (checkpoints, artifacts) live in an `aidex-storage` [`FileStore`] whose
/// blobs are SHA-256 addressed and verified on read. Records contain ids, timestamps,
/// and drive-relative data only. Every lifecycle change emits its `session.*` event
/// through the [`EventSink`] in the session's scope (see `store::publish`).
///
/// All methods take `&self`; one connection is serialised behind a mutex, so the
/// store is `Send + Sync` and may be shared as `Arc<SessionStore>`.
pub struct SessionStore {
    database: PathBuf,
    connection: Mutex<Connection>,
    pub(crate) blobs: FileStore,
    pub(crate) events: Arc<dyn EventSink>,
    host: HostFingerprint,
}

impl fmt::Debug for SessionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionStore")
            .field("database", &self.database)
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

impl SessionStore {
    /// Open (or create) the drive's session store and fingerprint the running host
    /// with the drive's salt (`<private_config>/host-salt`, created once).
    pub fn open(paths: &DrivePaths, events: Arc<dyn EventSink>) -> Result<Self, SessionError> {
        let host = HostFingerprint::for_drive(paths)?;
        Self::open_with_host(paths, events, host)
    }

    /// Open with an explicit current-host fingerprint (tests, other embedders).
    ///
    /// `<private_sessions>/` is created only through [`DrivePaths::ensure_dir`];
    /// symlinked database files (and SQLite sidecars) are refused, and a database
    /// written by a newer build is [`SessionError::Incompatible`].
    pub fn open_with_host(
        paths: &DrivePaths,
        events: Arc<dyn EventSink>,
        host: HostFingerprint,
    ) -> Result<Self, SessionError> {
        host.validate()?;
        let dir = paths.ensure_dir(&paths.sessions())?;
        let blob_dir = paths.ensure_dir(&dir.join(BLOB_DIR))?;
        let database = dir.join(SESSIONS_FILE);
        for suffix in ["", "-wal", "-shm", "-journal"] {
            refuse_link(Path::new(&format!("{}{suffix}", database.display())))?;
        }
        let connection = Connection::open(&database)?;
        connection.busy_timeout(Duration::from_secs(10))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        migration::migrate(&connection)?;
        let blobs = FileStore::open(plain_path(&blob_dir))?;
        Ok(Self {
            database,
            connection: Mutex::new(connection),
            blobs,
            events,
            host,
        })
    }

    /// The running host as fingerprinted at open.
    pub fn host(&self) -> &HostFingerprint {
        &self.host
    }

    pub fn database_path(&self) -> &Path {
        &self.database
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        // SQLite transactions keep the database consistent even if a holder panicked.
        self.connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Create a session in state `created` on the current host (`session.created`).
    pub fn create(&self, request: NewSession) -> Result<Session, SessionError> {
        let (title, _) = redact(bounded("title", &request.title, MAX_TITLE_BYTES, false)?);
        let id = EntityId::generate("ses").map_err(|error| {
            SessionError::Unavailable(format!("cannot generate a session id: {error}"))
        })?;
        let now = Timestamp::now();
        let session = Session {
            id,
            workspace_id: request.workspace_id,
            agent_id: request.agent_id,
            title,
            state: SessionState::Created,
            parent_session_id: request.parent_session_id,
            branched_from: None,
            model: request.model,
            created_at: now.clone(),
            updated_at: now,
            host_fingerprint: self.host.clone(),
            last_event_cursor: Cursor::ZERO,
            budget: BudgetUsage::default(),
            resumed_at: None,
        };
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(parent) = &session.parent_session_id {
            let parent = load_session(&transaction, parent)?;
            if parent.workspace_id != session.workspace_id {
                return Err(SessionError::InvalidArgument(
                    "a child session must belong to its parent's workspace".into(),
                ));
            }
        }
        insert_session(&transaction, &session)?;
        let pending = stage(
            &transaction,
            &session.id,
            EventKind::SessionCreated {
                agent_id: Some(session.agent_id.clone()),
                title: Some(session.title.clone()),
            },
        )?;
        transaction.commit()?;
        let mut session = session;
        self.publish(&mut connection, &mut session, vec![pending]);
        Ok(session)
    }

    pub fn get(&self, id: &EntityId) -> Result<Session, SessionError> {
        load_session(&self.lock(), id)
    }

    /// Sessions matching `filter`, newest first.
    pub fn list(&self, filter: &SessionFilter) -> Result<Vec<Session>, SessionError> {
        let limit = filter.effective_limit()?;
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions
             WHERE (?1 IS NULL OR workspace_id = ?1) AND (?2 IS NULL OR state = ?2)
             ORDER BY rowid DESC LIMIT ?3"
        ))?;
        let rows = statement.query_map(
            params![
                filter.workspace_id.as_ref().map(EntityId::as_str),
                filter.state.map(SessionStateKind::as_str),
                signed(limit as u64),
            ],
            session_row,
        )?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?.decode()?);
        }
        Ok(sessions)
    }

    /// Move a session through the state machine (see
    /// [`SessionStateKind::can_transition_to`]) and emit the matching event.
    ///
    /// Entering `running` records the current host as the host the session last ran
    /// on. Free text in `failed`/`interrupted` states is redacted and bounded.
    pub fn transition(&self, id: &EntityId, state: SessionState) -> Result<Session, SessionError> {
        let state = validate_state(state)?;
        let mut connection = self.lock();
        self.transition_locked(&mut connection, id, state, None)
    }

    /// Mark a running or waiting session `interrupted`; `recoverable` is computed from
    /// whether a checkpoint verifies (a corrupt latest one falls back to an older one).
    pub fn interrupt(&self, id: &EntityId, reason: &str) -> Result<Session, SessionError> {
        let (reason, _) = redact(bounded("reason", reason, MAX_REASON_BYTES, false)?);
        let mut connection = self.lock();
        let recoverable = self.has_valid_checkpoint(&connection, id)?;
        self.transition_locked(
            &mut connection,
            id,
            SessionState::Interrupted {
                reason,
                recoverable,
            },
            None,
        )
    }

    /// Transition with the connection held. `adopt_at` also records a resume.
    pub(crate) fn transition_locked(
        &self,
        connection: &mut Connection,
        id: &EntityId,
        state: SessionState,
        adopt_at: Option<&Timestamp>,
    ) -> Result<Session, SessionError> {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut session = load_session(&transaction, id)?;
        let from = session.kind();
        let to = state.kind();
        if !from.can_transition_to(to) {
            return Err(SessionError::InvalidTransition { from, to });
        }
        let now = Timestamp::now();
        let move_host = to == SessionStateKind::Running || adopt_at.is_some();
        if move_host {
            session.host_fingerprint = self.host.clone();
        }
        if let Some(at) = adopt_at {
            session.resumed_at = Some(at.clone());
        }
        session.state = state;
        session.updated_at = now;
        transaction.execute(
            "UPDATE sessions SET state = ?2, state_json = ?3, updated_at = ?4, host_json = ?5, resumed_at = ?6
             WHERE id = ?1",
            params![
                id.as_str(),
                to.as_str(),
                encode(&session.state)?,
                session.updated_at.as_str(),
                encode(&session.host_fingerprint)?,
                session.resumed_at.as_ref().map(Timestamp::as_str),
            ],
        )?;
        let pending = transition_event(from, &session.state)
            .map(|event| stage(&transaction, id, event))
            .transpose()?;
        transaction.commit()?;
        self.publish(connection, &mut session, pending.into_iter().collect());
        Ok(session)
    }

    /// Record the model the session now uses (`None` clears it). The router emits
    /// `model.selected` with its reason; the store emits nothing.
    pub fn set_model(
        &self,
        id: &EntityId,
        model: Option<ModelReference>,
    ) -> Result<Session, SessionError> {
        let connection = self.lock();
        rows::load_open_session(&connection, id)?;
        connection.execute(
            "UPDATE sessions SET model_json = ?2, updated_at = ?3 WHERE id = ?1",
            params![
                id.as_str(),
                model.as_ref().map(encode).transpose()?,
                Timestamp::now().as_str()
            ],
        )?;
        load_session(&connection, id)
    }

    /// Add resource use; counters saturate at `i64::MAX`.
    pub fn add_budget_usage(
        &self,
        id: &EntityId,
        usage: BudgetUsage,
    ) -> Result<Session, SessionError> {
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let session = load_session(&transaction, id)?;
        let total = session.budget.saturating_add(usage);
        transaction.execute(
            "UPDATE sessions SET input_tokens = ?2, output_tokens = ?3, tool_call_count = ?4, wall_ms = ?5, updated_at = ?6
             WHERE id = ?1",
            params![
                id.as_str(),
                signed(total.input_tokens),
                signed(total.output_tokens),
                signed(total.tool_calls),
                signed(total.wall_ms),
                Timestamp::now().as_str(),
            ],
        )?;
        let session = load_session(&transaction, id)?;
        transaction.commit()?;
        Ok(session)
    }

    /// Advance the session's event high-water mark (e.g. after the harness emitted
    /// `agent.*` events for it). The mark never moves backwards.
    pub fn set_event_cursor(&self, id: &EntityId, cursor: Cursor) -> Result<Session, SessionError> {
        let connection = self.lock();
        load_session(&connection, id)?;
        connection.execute(
            "UPDATE sessions SET last_event_cursor = ?2 WHERE id = ?1 AND last_event_cursor < ?2",
            params![id.as_str(), cursor_sql(cursor)],
        )?;
        load_session(&connection, id)
    }

    /// Number of rows of `table` belonging to `id` (for summaries).
    pub(crate) fn count(
        connection: &Connection,
        table: &'static str,
        id: &EntityId,
    ) -> Result<u64, SessionError> {
        let count: i64 = connection.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?1"),
            [id.as_str()],
            |row| row.get(0),
        )?;
        rows::unsigned(count, "count")
    }

    /// Whether `id` exists (without decoding it).
    pub(crate) fn exists(connection: &Connection, id: &EntityId) -> Result<bool, SessionError> {
        Ok(connection
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?1",
                [id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}

pub(crate) fn insert_session(
    connection: &Connection,
    session: &Session,
) -> Result<(), SessionError> {
    connection.execute(
        "INSERT INTO sessions (id, workspace_id, agent_id, title, state, state_json, parent_session_id,
            branched_from, model_json, created_at, updated_at, host_json, last_event_cursor)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            session.id.as_str(),
            session.workspace_id.as_str(),
            session.agent_id.as_str(),
            session.title,
            session.kind().as_str(),
            encode(&session.state)?,
            session.parent_session_id.as_ref().map(EntityId::as_str),
            session.branched_from.as_ref().map(EntityId::as_str),
            session.model.as_ref().map(encode).transpose()?,
            session.created_at.as_str(),
            session.updated_at.as_str(),
            encode(&session.host_fingerprint)?,
            cursor_sql(session.last_event_cursor),
        ],
    )?;
    Ok(())
}

/// Bound, validate, and redact the free text of a requested state.
fn validate_state(state: SessionState) -> Result<SessionState, SessionError> {
    Ok(match state {
        SessionState::Failed { code, message } => {
            validate_code("failure code", &code)?;
            let (message, _) = redact(bounded("message", &message, MAX_REASON_BYTES, true)?);
            SessionState::Failed { code, message }
        }
        SessionState::Interrupted {
            reason,
            recoverable,
        } => {
            let (reason, _) = redact(bounded("reason", &reason, MAX_REASON_BYTES, false)?);
            SessionState::Interrupted {
                reason,
                recoverable,
            }
        }
        other => other,
    })
}

/// `aidex-storage` walks every component of the path it is given, and Windows refuses
/// to stat a bare verbatim prefix (`\\?\C:`), so hand it the equivalent plain
/// spelling of a verbatim disk path (`C:\...`). Other paths are returned unchanged.
fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest)
            if rest.len() >= 3
                && rest.as_bytes()[0].is_ascii_alphabetic()
                && rest.as_bytes()[1] == b':'
                && rest.as_bytes()[2] == b'\\' =>
        {
            PathBuf::from(rest)
        }
        _ => path.to_path_buf(),
    }
}

fn refuse_link(path: &Path) -> Result<(), SessionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(SessionError::UnsafePath(path.to_path_buf()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
