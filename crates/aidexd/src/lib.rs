//! Local daemon lifecycle and authenticated IPC client.
//!
//! IPC uses a loopback TCP endpoint rooted at `.aidex/runtime/daemon.json` on the
//! selected drive. The endpoint carries a random 256-bit capability; clients must
//! also match contract version 1. Only handshake, status, and stop are exposed.
pub mod server;
