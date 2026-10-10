# Protocol versioning

The Rust types in `aidex-core` are canonical. `ContractVersion::CURRENT` is
version 1; every wire payload is carried in `Versioned<T>`. A receiver MUST
reject an unsupported version with `incompatible_version`; it MUST NOT guess,
coerce, or silently drop fields. Producers and consumers negotiate overlapping
`ContractOffer` minimum/maximum versions before using a contract.

Within a major contract version, adding optional fields is compatible; removing
or renaming fields, changing meaning or representation, tightening validation,
or changing required fields is breaking and requires a new version. Consumers
MUST reject unknown fields for closed DTOs and MUST NOT rely on enum fallback.
Breaking changes require a new schema file and explicit migration/conversion;
old versions remain readable only while explicitly supported. Secret material
is never part of a DTO; pass vault/secret references instead.

`protocol/schemas/contract-v1.schema.json` is generated from the Rust Serde
types. Regenerate with `cargo run -p aidex-core --bin
generate_contract_schema`; `cargo test -p aidex-core` checks the checked-in
schema against those types. Client code MUST be generated from this schema or
consume it directly, never maintain a parallel hand-authored definition.

## Daemon IPC contract version

The daemon transport has its own version, independent of `ContractVersion`:
IPC contract version 2 (`aidexd::CONTRACT_VERSION`) covers the endpoint file
format, framing, `aidex.handshake`, the error-code table, and the method set
documented in `protocol/commands/README.md`. Version 1 was the one-request-per-
connection protocol it replaces; a v2 daemon or client reports a v1 endpoint or
handshake as `incompatible_version` and never downgrades. Within version 2,
adding methods, optional params, result fields, or error codes is compatible;
renaming or removing methods, changing a code's meaning, or changing framing
requires version 3. Clients treat `method_not_found` as "daemon too old".
