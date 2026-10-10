# Protocol

Rust service DTOs in `aidex-core` define the canonical AiDEX contract; all payloads are explicitly versioned. `schemas/contract-v1.schema.json` is mechanically derived from those Serde types as the client boundary and checked for drift by the core crate tests. The JSON-RPC schema defines only the transport envelope.

Local frontends reach the daemon over IPC contract version 2: JSON-RPC 2.0, newline-delimited over authenticated loopback TCP, with a mandatory `aidex.handshake`, `<domain>.<verb>` method names, `<domain>.subscribe` streams, and a stable error-code table. See `commands/README.md`. The IPC contract version is independent of the DTO `ContractVersion`; see `versioning/README.md` for compatibility and migration policy.
