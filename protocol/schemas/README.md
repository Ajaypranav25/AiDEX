# Protocol schemas

`contract-v1.schema.json` is the generated JSON Schema client boundary for the Rust Serde DTOs in `aidex-core`; those Rust types are canonical. Regenerate it with `cargo run -p aidex-core --bin generate_contract_schema`. `cargo test -p aidex-core` deterministically compares generated and checked-in schema bytes and fails on drift. Do not edit generated schema by hand or declare a duplicated protocol definition canonical.

`json-rpc-2.0.schema.json` remains the transport envelope schema. Contract evolution and compatibility rules are in `protocol/versioning/README.md`.

The daemon's IPC handshake and error codes (contract version 2) are specified in `protocol/commands/README.md`; batches are not supported and ids are strings or integers, a subset of this envelope schema.
