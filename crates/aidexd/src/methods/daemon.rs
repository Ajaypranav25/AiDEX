//! Built-in `daemon.*` methods: status, ping, and graceful stop.

use crate::{
    router::{Router, RouterError},
    rpc::{CONTRACT_VERSION, expect_no_params, to_result},
    services::Services,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

pub const STATUS: &str = "daemon.status";
pub const PING: &str = "daemon.ping";
pub const STOP: &str = "daemon.stop";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonState {
    Ready,
    Stopping,
}

/// Result of `daemon.status`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DaemonStatus {
    pub state: DaemonState,
    pub pid: u32,
    pub contract_version: u32,
    pub version: String,
    pub drive_root: String,
    pub uptime_ms: u64,
}

/// Result of `daemon.stop`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StopAccepted {
    pub state: DaemonState,
}

pub fn status(services: &Services) -> DaemonStatus {
    DaemonStatus {
        state: if services.shutdown().is_requested() {
            DaemonState::Stopping
        } else {
            DaemonState::Ready
        },
        pid: services.pid(),
        contract_version: CONTRACT_VERSION,
        version: env!("CARGO_PKG_VERSION").into(),
        drive_root: services.drive_root().to_string_lossy().into_owned(),
        uptime_ms: u64::try_from(services.uptime().as_millis()).unwrap_or(u64::MAX),
    }
}

pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError> {
    let shared = Arc::clone(services);
    router.register(STATUS, move |_context, params| {
        expect_no_params(&params)?;
        to_result(&status(&shared))
    })?;
    router.register(PING, |_context, params| {
        expect_no_params(&params)?;
        Ok(json!({ "pong": true }))
    })?;
    let shared = Arc::clone(services);
    router.register(STOP, move |_context, params| {
        expect_no_params(&params)?;
        shared.shutdown().request();
        to_result(&StopAccepted {
            state: DaemonState::Stopping,
        })
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        errors::ErrorCode,
        lifecycle::ShutdownSignal,
        router::{CallContext, ClientInfo},
    };
    use serde_json::Value;

    #[test]
    fn status_ping_and_stop_report_and_request_shutdown() {
        let root = tempfile::tempdir().unwrap();
        let shutdown = ShutdownSignal::new();
        let services = Arc::new(Services::open(root.path(), shutdown.clone()).unwrap());
        let mut router = Router::new();
        register(&mut router, &services).unwrap();
        let context = CallContext::detached(ClientInfo::new("test", "1"), shutdown.clone());

        let status: DaemonStatus =
            serde_json::from_value(router.call(&context, STATUS, Value::Null).unwrap()).unwrap();
        assert_eq!(status.state, DaemonState::Ready);
        assert_eq!(status.pid, std::process::id());
        assert_eq!(status.contract_version, 2);
        assert_eq!(
            router.call(&context, PING, json!({})).unwrap(),
            json!({"pong": true})
        );
        assert_eq!(
            router
                .call(&context, STATUS, json!({"verbose": true}))
                .unwrap_err()
                .kind(),
            Some(ErrorCode::InvalidParams)
        );
        assert!(!shutdown.is_requested());
        assert_eq!(
            router.call(&context, STOP, Value::Null).unwrap(),
            json!({"state": "stopping"})
        );
        assert!(shutdown.is_requested());
        assert!(context.is_cancelled());
    }
}
