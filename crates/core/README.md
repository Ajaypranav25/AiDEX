# aidex-core

Small, dependency-light value types and versioned service-contract DTOs shared by every AiDEX Rust crate. These Serde types are canonical for the client boundary in [`protocol/schemas/contract-v1.schema.json`](../../protocol/schemas/contract-v1.schema.json); `cargo test -p aidex-core` fails if the generated schema drifts from the checked-in file.

## API

- `EntityId::new(value)` validates an opaque key (1-128 bytes of ASCII letters, digits, `-`, `_`, `.`, `:`). `EntityId::generate(prefix)` returns `<prefix>_<ULID>`: a 48-bit Unix-millisecond time and 80 bits from the operating-system RNG, encoded as 26 Crockford base32 characters. The prefix must be 1-16 ASCII lowercase letters (`IdError::InvalidPrefix`). If the RNG fails, the result is `IdError::Randomness`; weaker randomness is never substituted.
- `Timestamp::new(text)` accepts RFC 3339 UTC strings ending in `Z` with zero to nine fractional digits for years 0000-9999. `Timestamp::now()`, `from_system_time(SystemTime)`, and `from_unix_millis(i64)` produce canonical `YYYY-MM-DDTHH:MM:SS.mmmZ` text. `unix_millis()` reads any accepted form and truncates toward the past.
- `CancelToken` is a cloneable cancellation flag (`Arc<AtomicBool>`) with an optional deadline: `cancel()`, `with_deadline`/`with_timeout` (tighten only), `state() -> Option<CancelReason::{Cancelled, DeadlineExceeded}>`. Work checks it cooperatively; providers, tools, and the harness share it.
- `WorkspacePath`, `SecretRef`, `ContractVersion`, `ContractOffer`, `Versioned<T>`, `ServiceError`/`ServiceErrorCode`, and the request, response, and event DTOs make up the version-1 contract.

## Guarantees

- `Timestamp` ordering is chronological across fractional precisions, so it is safe for expiry comparisons. Different spellings of the same instant (`...01Z` and `...01.000Z`) stay unequal and are ordered by text as a tie-break, which keeps `Ord` consistent with `Eq`.
- For one prefix, generated IDs sort lexically by creation millisecond. IDs created in the same millisecond are unique with overwhelming probability but are not ordered.
- Calendar conversion is a self-contained proleptic Gregorian implementation (Howard Hinnant's days-from-civil); there is no date-time crate dependency.
- `SecretRef` carries vault and secret identifiers only, never secret values.

## Limitations

- Timestamps are UTC only. Leap seconds (`:60`) and offsets other than `Z` are rejected.
- `Timestamp::now()` saturates if the system clock falls outside years 0000-9999. `EntityId::generate` encodes time zero if the clock is before 1970.
