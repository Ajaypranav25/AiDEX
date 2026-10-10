//! The per-workspace local index: files, chunks with a full-text index, symbols,
//! optional vectors, and repository metadata, in one rebuildable SQLite database at
//! `DrivePaths::index()/<workspace-id>/index.sqlite3`.

mod chunk;
mod embed;
mod indexer;
mod repo;
mod schema;
mod search;
mod symbols;
mod walk;

use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use aidex_core::{EntityId, Timestamp};
use aidex_drive::DrivePaths;
use aidex_workspace::{ConfinedRoot, RepositoryInfo, WorkspaceHandle};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub use chunk::language_for;
pub use embed::Embedder;
pub use repo::RepositoryMetadata;
pub use schema::{INDEX_FILE, IndexRecovery, RecoveryKind, SCHEMA_VERSION};
pub use search::{
    DEFAULT_SEARCH_LIMIT, LineRange, MAX_QUERY_CHARS, MAX_SEARCH_LIMIT, Query, SearchHit,
    SearchMode, SymbolMatch,
};
pub use symbols::SymbolKind;
pub use walk::SkipCounts;

pub(crate) use search::{
    ChunkRow, chunk_at_line, chunk_by_ordinal, file_chunks, lexical_chunks, query_terms,
    symbol_rows,
};
pub(crate) use walk::mtime_ns as file_mtime_ns;

use crate::{ContextError, error::file_safe_id};

/// Bounds and switches for one index build.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexOptions {
    /// Larger files are skipped (default 1 MiB).
    pub max_file_bytes: u64,
    /// The walk stops after this many indexable files (default 200 000); the index
    /// is then marked `truncated`.
    pub max_files: usize,
    /// Files per committed batch (default 256); a cancelled build keeps every
    /// committed batch.
    pub batch_size: usize,
    /// Also honour the user's global Git excludes file (host-specific; default off
    /// so an index is the same on every host).
    pub global_excludes: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: 1024 * 1024,
            max_files: 200_000,
            batch_size: 256,
            global_excludes: false,
        }
    }
}

/// What to index: a workspace identity, its confined root, and its repository.
#[derive(Clone, Debug)]
pub struct IndexTarget {
    pub workspace_id: EntityId,
    pub root: ConfinedRoot,
    pub repository: Option<RepositoryInfo>,
}

impl IndexTarget {
    pub fn new(workspace_id: EntityId, root: ConfinedRoot) -> Self {
        Self {
            workspace_id,
            root,
            repository: None,
        }
    }

    #[must_use]
    pub fn with_repository(mut self, repository: Option<RepositoryInfo>) -> Self {
        self.repository = repository;
        self
    }

    /// The target for a registered workspace.
    pub fn from_handle(handle: &WorkspaceHandle) -> Self {
        Self {
            workspace_id: handle.id().clone(),
            root: handle.root().clone(),
            repository: handle.workspace().repository.clone(),
        }
    }
}

/// Which part of a build is running.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexPhase {
    #[default]
    Scanning,
    Indexing,
    Pruning,
    Finished,
}

/// Progress reported after each committed batch and at phase changes.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexProgress {
    pub phase: IndexPhase,
    /// Indexable files found by the walk (known once scanning finishes).
    pub total: u64,
    /// Files compared so far.
    pub processed: u64,
    /// Files (re)indexed so far.
    pub changed: u64,
    /// Files removed from the index so far.
    pub removed: u64,
}

/// How a build ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildOutcome {
    Completed,
    /// Stopped by cancellation; committed batches are kept and the next build
    /// resumes by skipping files that are already current. Nothing is pruned.
    Cancelled,
}

/// What a build did.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexReport {
    pub outcome: BuildOutcome,
    /// Build counter of this index.
    pub run: u64,
    /// Indexable files the walk found.
    pub files_seen: u64,
    /// New files indexed.
    pub added: u64,
    /// Changed files re-chunked.
    pub updated: u64,
    /// Files whose timestamp changed but content did not (not re-chunked).
    pub touched: u64,
    /// Files skipped because size and modification time were unchanged.
    pub unchanged: u64,
    /// Files removed (deleted, newly ignored, or no longer indexable).
    pub removed: u64,
    /// Indexed files in which credentials were redacted.
    pub redacted: u64,
    pub skipped: SkipCounts,
    /// The walk stopped at `max_files`.
    pub truncated: bool,
    pub elapsed_ms: u64,
}

/// Lifecycle state of an index.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexState {
    /// Never built.
    Empty,
    /// A build is running now.
    Building,
    /// A build was cancelled or interrupted; contents are usable but incomplete.
    Partial,
    /// The last build completed.
    Complete,
}

/// Counters and metadata of an index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexSummary {
    pub workspace_id: EntityId,
    pub state: IndexState,
    pub schema_version: i64,
    pub files: u64,
    pub chunks: u64,
    pub symbols: u64,
    pub vectors: u64,
    /// Total size of indexed files.
    pub bytes: u64,
    pub runs: u64,
    pub last_run_at: Option<Timestamp>,
    pub last_complete_at: Option<Timestamp>,
    pub truncated: bool,
    pub repository: Option<RepositoryMetadata>,
    /// Set when this index was rebuilt from scratch on open.
    pub recovery: Option<IndexRecovery>,
}

/// One workspace's index. `Send + Sync`: reads share one connection; a build
/// writes through its own connection, so searches see each committed batch.
pub struct Index {
    dir: PathBuf,
    workspace_id: EntityId,
    reader: Mutex<Connection>,
    building: AtomicBool,
    recovery: Option<IndexRecovery>,
}

pub(crate) mod meta_keys {
    pub const WORKSPACE: &str = "workspace_id";
    pub const STATE: &str = "state";
    pub const RUNS: &str = "runs";
    pub const LAST_RUN: &str = "last_run_at";
    pub const LAST_COMPLETE: &str = "last_complete_at";
    pub const TRUNCATED: &str = "truncated";
    pub const REPOSITORY: &str = "repository";
}

impl Index {
    /// Open (or create) the index stored in `dir`, which must exist. A corrupt or
    /// incompatible database is moved aside or reset and reported by
    /// [`Self::recovery`].
    pub fn open(dir: impl Into<PathBuf>, workspace_id: EntityId) -> Result<Self, ContextError> {
        let dir = dir.into();
        let (connection, recovery) = schema::open(&dir)?;
        match schema::meta_get(&connection, meta_keys::WORKSPACE)? {
            None => schema::meta_set(&connection, meta_keys::WORKSPACE, workspace_id.as_str())?,
            Some(owner) if owner == workspace_id.as_str() => {}
            Some(owner) => {
                return Err(ContextError::Index(format!(
                    "the index in `{}` belongs to workspace `{owner}`",
                    dir.display()
                )));
            }
        }
        Ok(Self {
            dir,
            workspace_id,
            reader: Mutex::new(connection),
            building: AtomicBool::new(false),
            recovery,
        })
    }

    /// Open the index of `workspace_id` in `DrivePaths::index()/<workspace-id>/`,
    /// creating the directory (symlinked components are refused).
    pub fn open_on_drive(paths: &DrivePaths, workspace_id: &EntityId) -> Result<Self, ContextError> {
        let name = file_safe_id(workspace_id)?;
        let dir = paths.ensure_dir(&paths.index().join(name))?;
        Self::open(dir, workspace_id.clone())
    }

    pub fn workspace_id(&self) -> &EntityId {
        &self.workspace_id
    }

    /// The database file.
    pub fn path(&self) -> PathBuf {
        self.dir.join(INDEX_FILE)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Why this index was rebuilt from scratch when it was opened, if it was.
    pub fn recovery(&self) -> Option<&IndexRecovery> {
        self.recovery.as_ref()
    }

    /// Whether a build is running on this handle.
    pub fn is_building(&self) -> bool {
        self.building.load(Ordering::SeqCst)
    }

    /// Index `target` incrementally: unchanged files (same size and modification
    /// time) are skipped, changed files are hashed and re-chunked only when their
    /// content changed, and files no longer present are removed once the walk has
    /// completed. `is_cancelled` is checked before every file; committed batches are
    /// kept. `progress` is called after every batch and phase change.
    pub fn build(
        &self,
        target: &IndexTarget,
        options: &IndexOptions,
        is_cancelled: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(&IndexProgress),
    ) -> Result<IndexReport, ContextError> {
        if target.workspace_id != self.workspace_id {
            return Err(ContextError::InvalidRequest(format!(
                "this index belongs to workspace `{}`, not `{}`",
                self.workspace_id, target.workspace_id
            )));
        }
        if self.building.swap(true, Ordering::SeqCst) {
            return Err(ContextError::IndexBusy(self.workspace_id.clone()));
        }
        let _guard = BuildingGuard(&self.building);
        indexer::build(&self.path(), target, options, is_cancelled, progress)
    }

    /// Counters, state, and repository metadata.
    pub fn summary(&self) -> Result<IndexSummary, ContextError> {
        let building = self.is_building();
        self.with_reader(|connection| {
            let count = |table: &str| -> Result<u64, ContextError> {
                connection
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map(|count| count.max(0) as u64)
                    .map_err(|error| ContextError::sqlite("cannot count index rows", error))
            };
            let meta = |key: &str| schema::meta_get(connection, key);
            let timestamp = |key: &str| -> Result<Option<Timestamp>, ContextError> {
                Ok(meta(key)?.and_then(|text| Timestamp::new(text).ok()))
            };
            let stored = meta(meta_keys::STATE)?;
            let state = match stored.as_deref() {
                _ if building => IndexState::Building,
                Some("complete") => IndexState::Complete,
                Some("partial" | "building") => IndexState::Partial,
                _ => IndexState::Empty,
            };
            let bytes: i64 = connection
                .query_row("SELECT coalesce(sum(size), 0) FROM files", [], |row| {
                    row.get(0)
                })
                .map_err(|error| ContextError::sqlite("cannot sum index sizes", error))?;
            Ok(IndexSummary {
                workspace_id: self.workspace_id.clone(),
                state,
                schema_version: SCHEMA_VERSION,
                files: count("files")?,
                chunks: count("chunks")?,
                symbols: count("symbols")?,
                vectors: count("vectors")?,
                bytes: bytes.max(0) as u64,
                runs: meta(meta_keys::RUNS)?
                    .and_then(|text| text.parse().ok())
                    .unwrap_or(0),
                last_run_at: timestamp(meta_keys::LAST_RUN)?,
                last_complete_at: timestamp(meta_keys::LAST_COMPLETE)?,
                truncated: meta(meta_keys::TRUNCATED)?.as_deref() == Some("1"),
                repository: meta(meta_keys::REPOSITORY)?
                    .and_then(|text| serde_json::from_str(&text).ok()),
                recovery: self.recovery.clone(),
            })
        })
    }

    /// Search the index (see [`Query`]). Works offline; results never include
    /// secret-named files and snippets are redacted.
    pub fn search(&self, query: &Query) -> Result<Vec<SearchHit>, ContextError> {
        self.with_reader(|connection| search::search(connection, query))
    }

    pub(crate) fn with_reader<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, ContextError>,
    ) -> Result<T, ContextError> {
        let connection = self
            .reader
            .lock()
            .map_err(|_| ContextError::Index("the index connection was poisoned".into()))?;
        work(&connection)
    }
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Index")
            .field("workspace_id", &self.workspace_id)
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

struct BuildingGuard<'a>(&'a AtomicBool);

impl Drop for BuildingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
