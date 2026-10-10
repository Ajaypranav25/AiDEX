# Repository indexing

Implemented by `aidex-context` (`crates/context`): the per-workspace local index (files, chunks with full-text search, symbols, optional vectors, repository metadata), ignore and secret exclusion, incremental and cancellable builds with corruption recovery, and offline search. See `crates/context/README.md`. Dependency graphs, git history search, and summaries are later work in that crate.
