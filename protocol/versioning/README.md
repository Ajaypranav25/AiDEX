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