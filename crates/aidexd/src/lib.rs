//! Local daemon lifecycle, authenticated IPC, and replayable event reads.
//!
//! IPC uses a loopback TCP endpoint rooted at `.aidex/runtime/daemon.json` on the
//! selected drive. The endpoint carries a random 256-bit capability; clients must
//! also match contract version 1. Event batches are read from the durable,
//! workspace-scoped SQLite log using a bounded acknowledged cursor. See
//! `src/server/README.md` for the wire contract.
pub mod errors;
pub mod rpc;
pub mod server;
