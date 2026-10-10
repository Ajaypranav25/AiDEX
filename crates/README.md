# Rust crate boundaries

## Workspace foundation

The Cargo workspace contains only code that has a concrete implementation today:

- `aidex-core` owns small cross-domain value types and the versioned contract DTOs: `EntityId` (with `EntityId::generate(prefix)` for prefixed ULIDs from the OS RNG), `Timestamp` (with `Timestamp::now()` and chronological ordering), `WorkspacePath`, `SecretRef`, and `ServiceError`. It has no domain or platform dependencies.
- `aidex-drive` validates portable relative paths, defines the schema-v2 root-level `aidex-drive.json` manifest (with an explicit, backed-up migration from v1), provides safe init/status operations that create only manifest-declared directories, and exposes `DrivePaths`, the resolver services use for their drive directories.
- `aidex-host` reports compile-time platform identifiers and best-effort process paths; it does not discover hardware or configure the operating system.
- `aidex-events` is the event contract: a closed typed catalog (`EventKind`, `{"type": "<dotted.name>", "data": {...}}`, see `protocol/events/README.md`), version-2 envelopes, one strictly increasing cursor per drive in a durable SQLite log, filtered replay, retention that reports expired cursors instead of gaps, subscriptions that page history from disk then deliver live events through a bounded queue (lagging subscribers catch up from disk; publishers never block), credential redaction (`redact_text`) and size bounds on every text field, and the `EventSink` trait (`MemoryEventSink`/`NullEventSink` for tests) through which domain crates emit. It depends only on `aidex-core` and embedded SQLite. `aidexd` keeps the log at `DrivePaths::sessions()/events.sqlite3`, emits `daemon.started`/`daemon.stopping`, and serves `events.replay`, `events.latest`, and the `events.subscribe` stream; `aidex events tail --follow` reconnects and resumes after its last printed cursor.
- `aidex-storage` uses embedded SQLite for transactional state (WAL, full synchronous commits, schema versioning, and one-time import of legacy file values) and SHA-256-addressed files for large artifacts. SQLite provides atomic metadata transactions without a service; blobs are synced before atomic rename and verified on read. The selected root and every managed path beneath it must be real directories or files (symlinks are rejected); host-owned ancestors such as macOS `/var` are resolved by the OS. This local persistence is not encrypted.
- `aidex-sessions` provides a process-local, in-memory session lifecycle catalog. It is not persistent and does not implement chat history.
- `aidex-sandbox` negotiates fail-closed OS process isolation (levels, mounts, network mode, resource limits) into an auditable `EnforcementPlan`, launches programs through its `aidex-sandbox-exec` helper (which must ship next to `aidexd`), and tracks every live process tree in a `ProcessRegistry` for cancel, shutdown, emergency stop, and drive removal. Windows uses Job Objects (level 1 maximum); Linux uses Landlock, rlimits, and namespaces (level 2); macOS uses `sandbox-exec`. It depends only on `aidex-core` plus safe OS binding crates; see `crates/sandbox/README.md` for the enforcement matrix.
- `aidex-policy` evaluates normalized action intents into allow/deny/sandbox/rate-limit/approval decisions: immutable hard safety rules, deny-wins layer precedence (global → session), built-in Safe/Balanced/Autonomous/Offline Secure profiles, tightening-only privacy, network, host-trust, and project-rule floors, closed-schema TOML policy files, conservative shell/network classifiers, and SHA-256 intent fingerprints. It is pure (no I/O during evaluation) and does not enforce anything; enforcement and approvals belong to the broker.
- `aidex-vault` is the portable encrypted vault (Argon2id → KEK → random master key; HKDF subkeys; XChaCha20-Poly1305 records and encrypted index; HMAC header; optional recovery key; atomic crash-safe writes; encrypted backup/restore) with scoped, expiring, revocable secret handles for `provider:`/`mcp:`/`process:`/`git:` consumers. `VaultService` is the daemon-owned, auto-locking wrapper; handle issuance/resolution are Rust APIs for the broker, not IPC methods. Independent security review is pending (`crates/vault/FORMAT.md`).
- `aidexd` owns the local daemon lifecycle and serves backend services as JSON-RPC 2.0 methods over capability-authenticated loopback IPC (contract version 2: newline-delimited, 1 MiB lines, mandatory `aidex.handshake`, `<domain>.<verb>` methods registered on a `Router`, bounded `<domain>.subscribe` notification streams, stable error codes; see `protocol/commands/README.md`). Services are composed once into `Services` and shared with `methods/<domain>.rs` adapters. State is rooted on the selected drive; the endpoint capability is protected by Unix file permissions or the current Windows user's DPAPI credential. Methods today: `daemon.status|ping|stop`, `events.replay|latest|subscribe`, `workspace.*`, and `vault.*` (the drive vault; handles stay in-process).status|ping|stop` and `events.replay|latest|subscribe`. The crate also provides the blocking `Client` used by frontends.
- `aidex-workspace` owns workspaces: a durable registry (`private_config/workspaces.sqlite3`) of explicitly selected host directories, drive workspaces (`workspaces/<id>/tree`, drive-relative so they survive drive-letter changes), and ephemeral copies (`workspaces/ephemeral/<id>/`) with baseline, review, conflict-checked selective apply, and marker-verified discard. `ConfinedRoot`/`WorkspaceHandle` resolve workspace-relative paths so traversal, symbolic links, and junctions cannot leave the root. It depends only on `aidex-core` and `aidex-drive`; `aidexd` serves it as `workspace.*` and the CLI as `aidex workspace` (alias `aidex project`).
- `aidex-cli` (`aidex`) is the command surface: clap definitions in `cli.rs`, one handler module per domain under `commands/`, a global `--json` flag, and stable exit codes (`exit.rs`, documented in `apps/cli/README.md`). Daemon-backed commands connect with `commands::daemon::connect`; `chat`/`run` still delegate to Oh My Pi.
- `aidex-bundle` is the portable bundle contract: the closed-schema `bundle.json` manifest, the drive's atomic `app/current.json` pointer (one entry per target, with the previous bundle as rollback candidate), integrity verification (`resolve_active`, which the launcher runs before executing anything), deterministic `.zip`/`.tar.gz` archives behind the `archive` feature, and crash-safe installation onto a drive (`install_with`, staged on the drive and switched atomically). It depends on `aidex-core` and `aidex-drive`; the updater (#27) is meant to build on it.
- `aidex-launcher` is the drive-root launcher (`AiDEX.exe`, or `./aidex` -> `app/launcher/<target>/aidex-launcher`): it finds the drive from its own path, verifies the drive manifest and the active bundle's critical binaries, and runs the bundle's `aidex` with `AIDEX_DRIVE_ROOT` set and the bundle's `bin/` first on `PATH`. It writes nothing and extracts nothing.
- `aidex-packager` is the developer/CI tool that builds verified bundles (`cargo build --release --locked`, license metadata from `cargo metadata`, `bundle.json`, archive, `SHA256SUMS`), installs them onto drives, and smoke-tests them; it is never shipped. See `portable/packaging/README.md`.

The daemon uses `aidex-drive` for the selected root. The CLI links the daemon IPC client and exposes `aidex daemon start|status|stop` and `aidex events tail|show`; the `aidexd` executable remains the lifecycle owner.

Dependency direction is inward toward small shared primitives: `aidex-core` has no AiDEX dependencies; `aidex-drive` depends only on `aidex-core`; `aidex-sessions` and `aidex-policy` may depend on `aidex-core`; `aidex-events` depends only on `aidex-core`, and domain crates emit through its `EventSink` trait; `aidex-storage` depends on embedded SQLite and SHA-256 primitives; `aidexd` composes infrastructure and domain crates; and `aidex-cli` depends on `aidexd` only for its client and DTOs. Domain crates should depend on lower-level contracts, never on the executable or UI. Avoid cycles; express cross-domain events at domain boundaries.

## Requested future domains (not Cargo packages yet)

These names reserve architecture concepts, not implemented crates. Add a package only when it contains a real API and behavior with a clear owner and dependency contract.

| Domain | Intended responsibility |
| --- | --- |
| `runtime` | Own lifecycle and coordination of local runtime services. |
| `inference` | Define inference requests, results, and execution orchestration. |
| `providers` | Adapt concrete model/runtime backends to inference contracts. |
| `router` | Select provider routes from policy and request capabilities. |
| `context` | Assemble bounded request context from eligible sources. |
| `memory` | Manage explicit persistent conversational memory. |
| `indexing` | Ingest, chunk, and index searchable content. |
| `chat` | Own conversation/message domain behavior and APIs. |
| `harness` | Define controlled execution environments for agent work. |
| `agent-adapters` | Translate external agent protocols into internal contracts. |
| `tools` | Describe and invoke capability-scoped tools. |
| `mcp` | Implement the Model Context Protocol boundary and transport. |
| `skills` | Discover, validate, and execute packaged agent skills. |
| `plugins` | Manage plugin metadata, lifecycle, and extension boundaries. |
| `broker` | Mediate requests to external services and resources. |
| `vault` | Implemented as `aidex-vault` (`crates/vault`); see above. |
| `identity` | Represent users, principals, and authentication identity. |
| `approvals` | Model explicit human approval requests and decisions. |
| `remote` | Define remote connectivity and synchronization contracts. |
| `cleanup` | Own safe cleanup of managed temporary and retained resources. |
| `updater` | Verify signatures, stage, self-test, and roll back application updates (#27), on top of `aidex-bundle`'s install and pointer. |

A likely dependency shape is shared core types → policy/identity/contracts → domain services → adapters/providers/transports → runtime composition and `aidexd`. Security-sensitive `vault`, `sandbox`, `approvals`, and `policy` contracts must be designed before callers rely on them. This table is a boundary map only; it does not promise those services exist.
