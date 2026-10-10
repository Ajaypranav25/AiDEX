# Rust crate boundaries

## Workspace foundation

The Cargo workspace contains only code that has a concrete implementation today:

- `aidex-core` owns small cross-domain value types (`EntityId`), with no domain or platform dependencies.
- `aidex-drive` validates portable relative paths, defines the schema-v1 root-level `aidex-drive.json` manifest, and provides safe init/status operations that create only manifest-declared directories.
- `aidex-host` reports compile-time platform identifiers and best-effort process paths; it does not discover hardware or configure the operating system.
- `aidex-events` broadcasts byte-payload events to in-process subscribers. It is not a durable event log or IPC transport.
- `aidex-storage` uses embedded SQLite for transactional state (WAL, full synchronous commits, schema versioning, and one-time import of legacy file values) and SHA-256-addressed files for large artifacts. SQLite provides atomic metadata transactions without a service; blobs are synced before atomic rename and verified on read. Managed paths reject symlink traversal. This local persistence is not encrypted.
- `aidex-sessions` provides a process-local, in-memory session lifecycle catalog. It is not persistent and does not implement chat history.
- `aidexd` owns the local daemon lifecycle and a versioned, capability-authenticated loopback IPC protocol. State is rooted on the selected drive; the endpoint capability is protected by Unix file permissions or the current Windows user's DPAPI credential. Its narrow API exposes readiness, status, and graceful shutdown.

The daemon uses `aidex-drive` for the selected root. The CLI links the daemon IPC client and exposes `aidex daemon start|status|stop`; the daemon executable remains the lifecycle owner.

Dependency direction is inward toward small shared primitives: `aidex-core` has no dependencies; `aidex-sessions` may depend on `aidex-core`; `aidex-storage` depends on embedded SQLite and SHA-256 primitives; and `aidexd` composes infrastructure crates. Domain crates should depend on lower-level contracts, never on the executable or UI. Avoid cycles; express cross-domain events at domain boundaries.

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
