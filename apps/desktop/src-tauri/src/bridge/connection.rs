//! One authenticated daemon connection per drive, reused across calls.
//!
//! Calls on a connection are sequential (the daemon handles one request at a time per
//! connection), so the slot is a mutex. A connection idle for longer than
//! [`HEALTH_CHECK_AFTER`] is checked with an idempotent `daemon.ping` before reuse; a
//! failed check reconnects before the real call is sent. A call that fails at the
//! connection level drops the connection, and the next call reconnects. A failed call
//! is never re-sent, so no request can run twice.

use crate::error::{kind, BridgeError};
use aidex_core::ServiceErrorCode;
use aidexd::{Client, ClientInfo};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

/// Client name presented in the daemon handshake.
pub const CLIENT_NAME: &str = "aidex-desktop";
/// Reused connections idle this long are pinged before the next call.
pub const HEALTH_CHECK_AFTER: Duration = Duration::from_secs(5);
const PING_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_METHOD_LEN: usize = 128;

struct Live {
    root: PathBuf,
    client: Client,
    last_used: Instant,
}

/// The shared call connection.
#[derive(Default)]
pub struct Connection {
    slot: Mutex<Option<Live>>,
}

impl Connection {
    /// Call `method` on the daemon serving `root`.
    pub fn call(&self, root: &Path, method: &str, params: Value) -> Result<Value, BridgeError> {
        validate_method(method)?;
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| BridgeError::internal("daemon connection lock poisoned"))?;
        if slot.as_ref().is_some_and(|live| live.root != root) {
            *slot = None;
        }
        if let Some(live) = slot.as_mut() {
            if live.last_used.elapsed() >= HEALTH_CHECK_AFTER
                && live
                    .client
                    .call_with_timeout("daemon.ping", Value::Null, PING_TIMEOUT)
                    .is_err()
            {
                *slot = None;
            }
        }
        if slot.is_none() {
            *slot = Some(Live {
                root: root.to_path_buf(),
                client: connect(root)?,
                last_used: Instant::now(),
            });
        }
        let Some(live) = slot.as_mut() else {
            return Err(BridgeError::internal("daemon connection unavailable"));
        };
        let outcome = live.client.call(method, params).map_err(BridgeError::from);
        live.last_used = Instant::now();
        if outcome
            .as_ref()
            .is_err_and(BridgeError::is_connection_failure)
        {
            *slot = None;
        }
        outcome
    }

    /// Drop the cached connection (drive changed, daemon stopped, app exiting).
    pub fn disconnect(&self) {
        if let Ok(mut slot) = self.slot.lock() {
            *slot = None;
        }
    }

    /// Whether a connection is currently cached.
    #[cfg(test)]
    pub fn is_connected(&self) -> bool {
        self.slot.lock().is_ok_and(|slot| slot.is_some())
    }
}

/// Open a new authenticated connection to the daemon serving `root`.
pub fn connect(root: &Path) -> Result<Client, BridgeError> {
    Client::connect_with(
        root,
        ClientInfo::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")),
    )
    .map_err(BridgeError::from)
}

/// Accept `<domain>.<verb>` names the UI may call. `aidex.*` methods (handshake,
/// unsubscribe) belong to the client and would desynchronize it.
pub fn validate_method(method: &str) -> Result<(), BridgeError> {
    let well_formed = !method.is_empty()
        && method.len() <= MAX_METHOD_LEN
        && method.contains('.')
        && !method.starts_with('.')
        && !method.ends_with('.')
        && method.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_')
        });
    if !well_formed {
        return Err(BridgeError::new(
            ServiceErrorCode::InvalidArgument,
            kind::INVALID_METHOD,
            format!("`{method}` is not a valid daemon method name"),
        ));
    }
    if method.starts_with("aidex.") {
        return Err(BridgeError::new(
            ServiceErrorCode::InvalidArgument,
            kind::INVALID_METHOD,
            format!("`{method}` is reserved for the connection itself"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_names_must_be_domain_verbs_outside_the_reserved_namespace() {
        for valid in [
            "daemon.status",
            "events.replay",
            "vault.status",
            "sessions.list_recent",
        ] {
            assert!(validate_method(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "",
            "status",
            ".status",
            "daemon.",
            "Daemon.Status",
            "daemon status",
            "aidex.handshake",
            "aidex.unsubscribe",
            &"a.".repeat(100),
        ] {
            let error = validate_method(invalid).unwrap_err();
            assert_eq!(error.kind, kind::INVALID_METHOD, "{invalid}");
            assert_eq!(error.code, "invalid_argument");
        }
    }

    #[test]
    fn calls_without_a_daemon_report_unavailable_and_cache_nothing() {
        let root = tempfile::tempdir().unwrap();
        let connection = Connection::default();
        let error = connection
            .call(root.path(), "daemon.status", Value::Null)
            .unwrap_err();
        assert_eq!(error.kind, kind::DAEMON_UNAVAILABLE);
        assert_eq!(error.code, "unavailable");
        assert!(!connection.is_connected());
    }
}
