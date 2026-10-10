//! `aidex-context`: local repository context for native agent tasks (spec §34,
//! §35, §54, §55, §76, §78, §91).
//!
//! - [`Index`]: a per-workspace, rebuildable SQLite index on the drive (files,
//!   chunks with an FTS5 full-text index, regex symbols, optional vectors,
//!   repository metadata). Builds are incremental, cancellable, batch-committed,
//!   and recover from corruption by moving the database aside and rebuilding.
//! - [`exclude`]: ignore rules plus secret-file and secret-content exclusion,
//!   applied to indexing, search, and pack assembly.
//! - [`Query`] / [`SearchHit`]: offline lexical (BM25), filename, and symbol search.
//! - [`ProjectRules`]: `.aidex/rules.toml` (tighten-only policy floors) and
//!   project instruction files.
//! - [`MemoryStore`] / [`FileMemoryStore`]: inspectable session, workspace, and
//!   user memory that refuses secret-like text.
//! - [`assemble`] / [`ContextPack`]: budgeted packs of relevant chunks, rules, and
//!   memory, wrapped in untrusted-data delimiters.
//! - [`ContextService`]: the daemon-owned manager with one background indexing job
//!   per workspace.
//!
//! Nothing here uses the network.

mod error;
pub mod exclude;
mod fsio;
mod index;
mod tokens;

pub use error::ContextError;
pub use index::{
    BuildOutcome, DEFAULT_SEARCH_LIMIT, Embedder, INDEX_FILE, Index, IndexOptions, IndexPhase,
    IndexProgress, IndexRecovery, IndexReport, IndexState, IndexSummary, IndexTarget, LineRange,
    MAX_QUERY_CHARS, MAX_SEARCH_LIMIT, Query, RecoveryKind, RepositoryMetadata, SCHEMA_VERSION,
    SearchHit, SearchMode, SkipCounts, SymbolKind, SymbolMatch, language_for,
};
pub use tokens::{CharEstimator, TokenEstimator};
