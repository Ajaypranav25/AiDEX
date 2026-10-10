# Rust crate boundaries

## Workspace foundation

The Cargo workspace contains only code that has a concrete implementation today:

- `aidex-core` owns small cross-domain value types and the versioned contract DTOs: `EntityId` (with `EntityId::generate(prefix)` for prefixed ULIDs from the OS RNG), `Timestamp` (with `Timestamp::now()` and chronological ordering), `WorkspacePath`, `SecretRef`, and `ServiceError`. It has no domain or platform dependencies.
- `aidex-drive` validates portable relative paths, defines the schema-v2 root-level `aidex-drive.json` manifest (with an explicit, backed-up migration from v1), provides safe init/status operations that create only manifest-declared directories, and exposes `DrivePaths`, the resolver services use for their drive directories.
- `aidex-host` reports compile-time platform identifiers and best-effort process paths; it does not discover hardware or configure the operating system.
- `aidex-events` defines versioned typed workspace/session transitions with SQLite persistence, ordered cursors, explicit retention gaps, and bounded subscriptions. `aidexd` serves bounded replay from the selected drive's event database as the `events.replay` JSON-RPC method; `aidex events --follow` reconnects from its last flushed cursor.
- `aidex-storage` uses embedded SQLite for transactional state (WAL, full synchronous commits, schema versioning, and one-time import of legacy file values) and SHA-256-addressed files for large artifacts. SQLite provides atomic metadata transactions without a service; blobs are synced before atomic rename and verified on read. The selected root and every managed path beneath it must be real directories or files (symlinks are rejected); host-owned ancestors such as macOS `/var` are resolved by the OS. This local persistence is not encrypted.
- `aidex-sessions` provides a process-local, in-memory session lifecycle catalog. It is not persistent and does not implement chat history.
- `aidexd` owns the local daemon lifecycle and serves backend services as JSON-RPC 2.0 methods over capability-authenticated loopback IPC (contract version 2: newline-delimited, 1 MiB lines, mandatory `aidex.handshake`, `<domain>.<verb>` methods registered on a `Router`, bounded `<domain>.subscribe` notification streams, stable error codes; see `protocol/commands/README.md`). Services are composed once into `Services` and shared with `methods/<domain>.rs` adapters. State is rooted on the selected drive; the endpoint capability is protected by Unix file permissions or the current Windows user's DPAPI credential. Methods today: `daemon.status|ping|stop` and `events.replay`. The crate also provides the blocking `Client` used by frontends.
- `aidex-cli` (`aidex`) is the command surface: clap definitions in `cli.rs`, one handler module per domain under `commands/`, a global `--json` flag, and stable exit codes (`exit.rs`, documented in `apps/cli/README.md`). Daemon-backed commands connect with `commands::daemon::connect`; `chat`/`run` still delegate to Oh My Pi.

The daemon uses `aidex-drive` for the selected root. The CLI links the daemon IPC client and exposes `aidex daemon start|status|stop` and `aidex events`; the `aidexd` executable remains the lifecycle owner.

Dependency direction is inward toward small shared primitives: `aidex-core` has no AiDEX dependencies; `aidex-drive` depends only on `aidex-core`; `aidex-sessions` may depend on `aidex-core`; `aidex-storage` depends on embedded SQLite and SHA-256 primitives; `aidexd` composes infrastructure and domain crates; and `aidex-cli` depends on `aidexd` only for its client and DTOs. Domain crates should depend on lower-level contracts, never on the executable or UI. Avoid cycles; express cross-domain events at domain boundaries.

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
| `policy` | Evaluate authorization and execution policy decisions. |
| `sandbox` | Enforce process/resource isolation for untrusted work. |
| `vault` | Store and retrieve protected secrets and credentials. |
| `identity` | Represent users, principals, and authentication identity. |
| `approvals` | Model explicit human approval requests and decisions. |
| `remote` | Define remote connectivity and synchronization contracts. |
| `cleanup` | Own safe cleanup of managed temporary and retained resources. |
| `updater` | Verify, stage, and apply application updates. |

A likely dependency shape is shared core types → policy/identity/contracts → domain services → adapters/providers/transports → runtime composition and `aidexd`. Security-sensitive `vault`, `sandbox`, `approvals`, and `policy` contracts must be designed before callers rely on them. This table is a boundary map only; it does not promise those services exist.
