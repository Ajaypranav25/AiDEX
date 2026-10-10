//! Bridge behaviour against an in-process daemon on a temporary drive.

use super::{Bridge, EndedPayload, NotificationEmitter, NotificationPayload};
use crate::error::kind;
use aidexd::{Daemon, DaemonHandle, DaemonOptions};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const PATIENCE: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Recorder {
    notifications: Mutex<Vec<NotificationPayload>>,
    ended: Mutex<Vec<EndedPayload>>,
}

impl NotificationEmitter for Recorder {
    fn notification(&self, payload: NotificationPayload) {
        self.notifications.lock().unwrap().push(payload);
    }
    fn ended(&self, payload: EndedPayload) {
        self.ended.lock().unwrap().push(payload);
    }
}

fn temp_drive() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    aidex_drive::init_with(
        root.path(),
        aidex_drive::InitOptions {
            require_removable: false,
        },
    )
    .unwrap();
    root
}

/// A daemon with `test.count` (emits `count` notifications, then ends) and
/// `test.forever` (ticks until unsubscribed).
fn start_daemon(root: &Path) -> DaemonHandle {
    let options = DaemonOptions {
        poll_interval: Duration::from_millis(10),
        ..DaemonOptions::default()
    };
    let mut daemon = Daemon::with_options(root, options).unwrap();
    let router = daemon.router_mut();
    router
        .register_stream("test.count", |_, params, sink| {
            let count = params["count"].as_u64().unwrap_or(0);
            thread::spawn(move || {
                for index in 0..count {
                    if sink
                        .notify("test.event", json!({ "index": index }))
                        .is_err()
                    {
                        return;
                    }
                }
            });
            Ok(json!({ "count": count }))
        })
        .unwrap();
    router
        .register_stream("test.forever", |_, _, sink| {
            thread::spawn(move || {
                while sink.notify("test.tick", Value::Null).is_ok() {
                    thread::sleep(Duration::from_millis(20));
                }
            });
            Ok(Value::Null)
        })
        .unwrap();
    daemon.start().unwrap()
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !condition() {
        assert!(Instant::now() < deadline, "condition not met in time");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn calls_proxy_any_method_and_keep_daemon_error_codes() {
    let root = temp_drive();
    let _daemon = start_daemon(root.path());
    let bridge = Bridge::default();

    let status = bridge
        .connection
        .call(root.path(), "daemon.status", Value::Null)
        .unwrap();
    assert_eq!(status["state"], "ready");
    assert_eq!(status["contract_version"], 2);
    assert!(bridge.connection.is_connected());

    let replay = bridge
        .connection
        .call(
            root.path(),
            "events.replay",
            json!({ "workspace_id": "workspace", "after": 0 }),
        )
        .unwrap();
    assert_eq!(replay, json!({ "events": [] }));

    let missing = bridge
        .connection
        .call(root.path(), "vault.status", Value::Null)
        .unwrap_err();
    assert_eq!(missing.kind, "method_not_found");
    assert_eq!(missing.code, "not_found");
    assert_eq!(missing.rpc_code, Some(-32601));
    let invalid = bridge
        .connection
        .call(root.path(), "daemon.status", json!({ "verbose": true }))
        .unwrap_err();
    assert_eq!(invalid.kind, "invalid_params");
    assert!(
        bridge.connection.is_connected(),
        "an RPC error keeps the connection"
    );
}

#[test]
fn a_restarted_daemon_is_reconnected_without_resending_failed_calls() {
    let root = temp_drive();
    let bridge = Bridge::default();
    let daemon = start_daemon(root.path());
    bridge
        .connection
        .call(root.path(), "daemon.ping", Value::Null)
        .unwrap();
    daemon.stop();
    daemon.wait().unwrap();

    let error = bridge
        .connection
        .call(root.path(), "daemon.ping", Value::Null)
        .unwrap_err();
    assert!(error.is_connection_failure(), "{error}");
    assert!(
        !bridge.connection.is_connected(),
        "a dead connection is dropped"
    );

    let _restarted = start_daemon(root.path());
    let pong = bridge
        .connection
        .call(root.path(), "daemon.ping", Value::Null)
        .unwrap();
    assert_eq!(pong, json!({ "pong": true }));
}

#[test]
fn subscriptions_forward_notifications_then_report_the_end_once() {
    let root = temp_drive();
    let _daemon = start_daemon(root.path());
    let bridge = Bridge::default();
    let recorder = Arc::new(Recorder::default());

    let subscribed = bridge
        .subscriptions
        .subscribe(
            root.path(),
            "test.count",
            json!({ "count": 3 }),
            recorder.clone(),
        )
        .unwrap();
    assert_eq!(subscribed.initial, json!({ "count": 3 }));
    wait_until(|| !recorder.ended.lock().unwrap().is_empty());

    let notifications = recorder.notifications.lock().unwrap();
    let indexes: Vec<_> = notifications
        .iter()
        .map(|payload| payload.data["index"].as_u64().unwrap())
        .collect();
    assert_eq!(indexes, [0, 1, 2]);
    assert!(notifications.iter().all(|payload| {
        payload.subscription == subscribed.subscription && payload.method == "test.event"
    }));
    let ended = recorder.ended.lock().unwrap();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].subscription, subscribed.subscription);
    assert!(ended[0].error.is_none());
    assert_eq!(
        bridge.subscriptions.len(),
        0,
        "an ended stream frees its slot"
    );
}

#[test]
fn unsubscribing_stops_forwarding_and_emits_no_end_event() {
    let root = temp_drive();
    let _daemon = start_daemon(root.path());
    let bridge = Bridge::default();
    let recorder = Arc::new(Recorder::default());

    let subscribed = bridge
        .subscriptions
        .subscribe(root.path(), "test.forever", Value::Null, recorder.clone())
        .unwrap();
    wait_until(|| recorder.notifications.lock().unwrap().len() >= 2);
    assert!(bridge.subscriptions.unsubscribe(&subscribed.subscription));
    assert_eq!(bridge.subscriptions.len(), 0);

    // Let the worker notice the flag, then make sure nothing else arrives.
    thread::sleep(super::subscriptions::POLL_INTERVAL * 3);
    let settled = recorder.notifications.lock().unwrap().len();
    thread::sleep(super::subscriptions::POLL_INTERVAL * 2);
    assert_eq!(recorder.notifications.lock().unwrap().len(), settled);
    assert!(recorder.ended.lock().unwrap().is_empty());
}

#[test]
fn unknown_stream_methods_fail_the_subscribe_call() {
    let root = temp_drive();
    let _daemon = start_daemon(root.path());
    let bridge = Bridge::default();
    let error = bridge
        .subscriptions
        .subscribe(
            root.path(),
            "nothing.subscribe",
            Value::Null,
            Arc::new(Recorder::default()),
        )
        .unwrap_err();
    assert_eq!(error.kind, "method_not_found");
    assert_eq!(bridge.subscriptions.len(), 0);
    assert_ne!(error.kind, kind::SUBSCRIPTION_LIMIT);
}
