//! `events.*` methods over the durable `aidex-events` log.
//!
//! `events.replay` ports the contract-v1 `events` command (PR #40) to IPC v2:
//! one bounded batch strictly after the caller's acknowledged cursor.

use crate::{
    errors::RpcError,
    router::{Router, RouterError},
    rpc::{parse_params, to_result},
    services::Services,
};
use aidex_core::ServiceErrorCode;
use aidex_events::{Event, EventError, MAX_BATCH};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

pub const REPLAY: &str = "events.replay";

/// Params of `events.replay`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReplayParams {
    pub workspace_id: String,
    /// Last acknowledged sequence; `0` replays from the beginning.
    pub after: u64,
    /// Batch size from 1 to [`MAX_BATCH`] (default).
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    MAX_BATCH
}

/// Result of `events.replay`: events in ascending sequence order.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReplayBatch {
    pub events: Vec<Event>,
}

/// Stable category for an event-log error.
pub fn event_error_code(error: &EventError) -> ServiceErrorCode {
    match error {
        EventError::Storage(_) => ServiceErrorCode::Internal,
        EventError::CursorExpired { .. } => ServiceErrorCode::NotFound,
        EventError::InvalidLimit | EventError::InvalidCursor | EventError::InvalidIdentifier => {
            ServiceErrorCode::InvalidArgument
        }
    }
}

fn to_rpc(error: EventError) -> RpcError {
    match error {
        EventError::CursorExpired {
            requested,
            earliest,
        } => RpcError::service(ServiceErrorCode::NotFound, error.to_string()).with_data(json!({
            "reason": "cursor_expired",
            "requested": requested,
            "earliest": earliest,
        })),
        EventError::InvalidLimit | EventError::InvalidCursor | EventError::InvalidIdentifier => {
            RpcError::invalid_params(error.to_string())
        }
        EventError::Storage(_) => RpcError::service(event_error_code(&error), error.to_string()),
    }
}

pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError> {
    let shared = Arc::clone(services);
    router.register(REPLAY, move |_context, params| {
        let params: ReplayParams = parse_params(params)?;
        let events = shared
            .events()
            .read_after(&params.workspace_id, params.after, params.limit)
            .map_err(to_rpc)?;
        to_result(&ReplayBatch { events })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        errors::ErrorCode,
        lifecycle::ShutdownSignal,
        router::{CallContext, ClientInfo},
    };
    use aidex_events::EventKind;

    fn fixture(published: &[&str]) -> (tempfile::TempDir, Router, CallContext) {
        let root = crate::services::testing::temp_drive();
        let shutdown = ShutdownSignal::new();
        let services = Arc::new(Services::open(root.path(), shutdown.clone()).unwrap());
        for task_id in published {
            services
                .events()
                .publish(
                    "workspace",
                    None,
                    EventKind::TaskStarted {
                        task_id: (*task_id).into(),
                    },
                )
                .unwrap();
        }
        let mut router = Router::new();
        register(&mut router, &services).unwrap();
        let context = CallContext::detached(ClientInfo::new("test", "1"), shutdown);
        (root, router, context)
    }

    #[test]
    fn replay_returns_events_after_the_cursor_in_order() {
        let (_root, router, context) = fixture(&["a", "b", "c"]);
        let batch: ReplayBatch = serde_json::from_value(
            router
                .call(
                    &context,
                    REPLAY,
                    json!({"workspace_id": "workspace", "after": 1}),
                )
                .unwrap(),
        )
        .unwrap();
        let sequences: Vec<_> = batch.events.iter().map(|event| event.sequence).collect();
        assert_eq!(sequences, [2, 3]);
        let limited: ReplayBatch = serde_json::from_value(
            router
                .call(
                    &context,
                    REPLAY,
                    json!({"workspace_id": "workspace", "after": 0, "limit": 1}),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(limited.events.len(), 1);
    }

    #[test]
    fn invalid_queries_are_invalid_params() {
        let (_root, router, context) = fixture(&[]);
        for params in [
            json!({"workspace_id": "workspace", "after": 0, "limit": 0}),
            json!({"workspace_id": "workspace", "after": 0, "limit": MAX_BATCH + 1}),
            json!({"workspace_id": "bad id!", "after": 0}),
            json!({"workspace_id": "workspace"}),
            json!({"workspace_id": "workspace", "after": 0, "extra": 1}),
        ] {
            let error = router.call(&context, REPLAY, params.clone()).unwrap_err();
            assert_eq!(error.kind(), Some(ErrorCode::InvalidParams), "{params}");
        }
    }
}
