# Rust crate boundaries

## Workspace foundation

The Cargo workspace contains only code that has a concrete implementation today:

- `aidex-core` owns small cross-domain value types (`EntityId`), with no domain or platform dependencies.
- `aidex-drive` validates portable relative paths, defines the schema-v1 root-level `aidex-drive.json` manifest, and provides safe init/status operations that create only manifest-declared directories.
- `aidex-host` reports compile-time platform identifiers and best-effort process paths; it does not discover hardware or configure the operating system.
- `aidex-events` broadcasts byte-payload events to in-process subscribers. It is not a durable event log or IPC transport.
- `aidex-storage` provides a file-backed key/value API and recursive key listing. It is not an encrypted database or transactional store.
- `aidex-sessions` provides a process-local, in-memory session lifecycle catalog. It is not persistent and does not implement chat history.
- `aidexd` is the daemon binary entry-point shell. Its current `host` command is diagnostic only; no daemon/service is started.

Dependency direction is inward toward small shared primitives: `aidex-core` has no dependencies; `aidex-sessions` may depend on `aidex-core`; leaf infrastructure crates (`drive`, `host`, `events`, `storage`) currently have no crate dependencies; and the `aidexd` entry point composes infrastructure. Domain crates should depend on lower-level contracts, never on the executable or UI. Avoid introducing cycles; cross-domain events should be expressed at domain boundaries rather than coupling peer domains.

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
