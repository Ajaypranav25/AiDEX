//! `events.replay` over IPC v2 (ported from the contract-v1 `events` command).

mod common;

use aidex_events::{EventKind, EventLog};
use aidexd::{
    Client, ErrorCode, endpoint,
    methods::events::{REPLAY, ReplayBatch},
    services::EVENT_FILE,
};
use common::{options, start};
use serde_json::json;
use std::path::Path;

fn event_log(root: &Path, retention: u64) -> EventLog {
    let runtime = endpoint::ensure_state_dir(root).unwrap();
    EventLog::open(runtime.join(EVENT_FILE), retention).unwrap()
}

fn replay(client: &mut Client, after: u64, limit: usize) -> Vec<u64> {
    client
        .call_as::<ReplayBatch>(
            REPLAY,
            json!({ "workspace_id": "workspace", "after": after, "limit": limit }),
        )
        .unwrap()
        .events
        .into_iter()
        .map(|event| event.sequence)
        .collect()
}

#[test]
fn authenticated_ipc_replays_from_last_acknowledged_cursor_across_restart() {
    let root = tempfile::tempdir().unwrap();
    {
        let log = event_log(root.path(), 20);
        log.publish(
            "workspace",
            Some("session".into()),
            EventKind::SessionStarted,
        )
        .unwrap();
        log.publish(
            "workspace",
            Some("session".into()),
            EventKind::SessionFinished,
        )
        .unwrap();
    }
    let daemon = start(root.path(), options(), |_| {});
    let mut client = Client::connect(root.path(), "test-events").unwrap();
    assert_eq!(replay(&mut client, 0, 1), [1]);
    assert_eq!(replay(&mut client, 1, 1), [2]);
    assert!(replay(&mut client, 2, 1).is_empty());
    drop(client);
    daemon.stop();
    daemon.wait().unwrap();

    let restarted = start(root.path(), options(), |_| {});
    let mut client = Client::connect(root.path(), "test-events").unwrap();
    assert_eq!(replay(&mut client, 0, 2), [1, 2]);
    restarted.stop();
    restarted.wait().unwrap();
}

#[test]
fn expired_event_cursor_is_a_permanent_not_found_error() {
    let root = tempfile::tempdir().unwrap();
    {
        let log = event_log(root.path(), 1);
        for task_id in ["first", "second"] {
            log.publish(
                "workspace",
                None,
                EventKind::TaskStarted {
                    task_id: task_id.into(),
                },
            )
            .unwrap();
        }
    }
    let daemon = start(root.path(), options(), |_| {});
    let mut client = common::client(daemon.endpoint());
    let error = client
        .call(
            REPLAY,
            json!({ "workspace_id": "workspace", "after": 0, "limit": 1 }),
        )
        .unwrap_err();
    assert_eq!(error.rpc_kind(), Some(ErrorCode::NotFound));
    assert!(error.to_string().contains("expired"), "{error}");
    let aidexd::ClientError::Rpc { data, .. } = error else {
        unreachable!()
    };
    assert_eq!(data.unwrap()["reason"], "cursor_expired");
    assert_eq!(replay(&mut client, 1, 1), [2]);
}
