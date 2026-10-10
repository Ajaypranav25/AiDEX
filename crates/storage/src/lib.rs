//! Crash-safe key/value and content-addressed artifact storage beneath a drive root.
//!
//! SQLite provides transactional metadata, WAL journaling, full synchronous commits, and
//! schema migrations. Content-addressed artifact files are synced before atomic rename and
//! verified by hash on read. Legacy file-backed values are imported once without replacing
//! committed database values.

mod artifacts;
mod error;
mod migration;
mod paths;
mod store;

pub use error::StoreError;
pub use store::FileStore;

#[cfg(test)]
mod tests;
