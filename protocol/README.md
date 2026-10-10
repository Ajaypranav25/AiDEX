# Protocol

Rust service DTOs in `aidex-core` define the canonical AiDEX contract; all payloads are explicitly versioned. `schemas/contract-v1.schema.json` is mechanically derived from those Serde types as the client boundary and checked for drift by the core crate tests. The JSON-RPC schema defines only the transport envelope. See `versioning/README.md` for compatibility and migration policy.
