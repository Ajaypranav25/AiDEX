//! Wire-level behaviour of the IPC v2 transport.

mod common;

use aidexd::{ClientError, ErrorCode, SinkError, TransportConfig};
use common::{PATIENCE, Raw, client, eventually, handshake, options, request, start};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

fn error_code(reply: &Value) -> i32 {
    reply["error"]["code"]
        .as_i64()
        .and_then(|code| i32::try_from(code).ok())
        .unwrap_or_else(|| panic!("expected an error reply, got {reply}"))
}

#[test]
fn first_message_must_be_the_handshake() {
    let root = common::temp_drive();
    let daemon = start(root.path(), options(), |_| {});
    let mut raw = Raw::connect(daemon.endpoint());
    raw.send(&request(1, "daemon.status", Value::Null));
    let reply = raw.recv();
    assert_eq!(error_code(&reply), ErrorCode::HandshakeRequired.code());
    assert_eq!(reply["id"], 1);
    raw.expect_closed();

    let mut notification_first = Raw::connect(daemon.endpoint());
    let mut message = handshake(daemon.endpoint());
    message.as_object_mut().unwrap().remove("id");
    notification_first.send(&message);
    let reply = notification_first.recv();
    assert_eq!(error_code(&reply), ErrorCode::HandshakeRequired.code());
    assert_eq!(reply["id"], Value::Null);
    notification_first.expect_closed();
}

#[test]
fn wrong_capability_is_unauthorized_and_closed() {
    let root = common::temp_drive();
    let daemon = start(root.path(), options(), |_| {});
    let mut message = handshake(daemon.endpoint());
    let wrong = if daemon.endpoint().token.expose().starts_with('0') {
        "1".repeat(64)
    } else {
        "0".repeat(64)
    };
    message["params"]["token"] = json!(wrong);
    let mut raw = Raw::connect(daemon.endpoint());
    raw.send(&message);
    let reply = raw.recv();
    assert_eq!(error_code(&reply), ErrorCode::Unauthorized.code());
    assert!(!reply.to_string().contains(daemon.endpoint().token.expose()));
    raw.expect_closed();
}

#[test]
fn incompatible_contract_version_lists_supported_versions_and_closes() {
    let root = common::temp_drive();
    let daemon = start(root.path(), options(), |_| {});
    let mut message = handshake(daemon.endpoint());
    message["params"]["contract_version"] = json!(1);
    let mut raw = Raw::connect(daemon.endpoint());
    raw.send(&message);
    let reply = raw.recv();
    assert_eq!(error_code(&reply), ErrorCode::IncompatibleVersion.code());
    assert_eq!(reply["error"]["data"]["supported"], json!([2]));
    raw.expect_closed();
}

#[test]
fn silent_connections_are_closed_after_the_handshake_timeout() {
    let root = common::temp_drive();
    let mut options = options();
    options.transport.handshake_timeout = Duration::from_millis(200);
    let daemon = start(root.path(), options, |_| {});
    let mut raw = Raw::connect(daemon.endpoint());
    let reply = raw.recv();
    assert_eq!(error_code(&reply), ErrorCode::HandshakeRequired.code());
    raw.expect_closed();
}

#[test]
fn oversized_lines_are_rejected_and_the_connection_closed() {
    let root = common::temp_drive();
    let mut options = options();
    options.transport.max_message_bytes = 64 * 1024;
    let daemon = start(root.path(), options, |_| {});

    let mut huge_handshake = Raw::connect(daemon.endpoint());
    let mut padded = vec![b' '; 8 * 1024];
    padded.extend_from_slice(b"x\n");
    huge_handshake.send_bytes(&padded);
    let reply = huge_handshake.recv();
    assert_eq!(error_code(&reply), ErrorCode::MessageTooLarge.code());
    huge_handshake.expect_closed();

    let mut raw = Raw::connect(daemon.endpoint());
    raw.send(&handshake(daemon.endpoint()));
    raw.recv();
    let mut line = vec![b'x'; 100 * 1024];
    line.push(b'\n');
    raw.send_bytes(&line);
    let reply = raw.recv();
    assert_eq!(error_code(&reply), ErrorCode::MessageTooLarge.code());
    assert_eq!(reply["id"], Value::Null);
    raw.expect_closed();
}

#[test]
fn malformed_and_invalid_messages_get_errors_on_an_open_connection() {
    let root = common::temp_drive();
    let daemon = start(root.path(), options(), |_| {});
    let mut raw = Raw::authenticated(daemon.endpoint());

    raw.send_bytes(b"{not json\n");
    let reply = raw.recv();
    assert_eq!(error_code(&reply), -32700);
    assert_eq!(reply["id"], Value::Null);

    raw.send(&json!({"jsonrpc": "1.0", "id": 5, "method": "daemon.ping"}));
    let reply = raw.recv();
    assert_eq!(error_code(&reply), -32600);
    assert_eq!(reply["id"], 5);

    raw.send(&json!([request(6, "daemon.ping", Value::Null)]));
    assert_eq!(error_code(&raw.recv()), -32600);

    raw.send(&handshake(daemon.endpoint()));
    assert_eq!(error_code(&raw.recv()), -32600);

    raw.send(&request(7, "nope.missing", Value::Null));
    let reply = raw.recv();
    assert_eq!(error_code(&reply), -32601);
    assert_eq!(reply["id"], 7);

    raw.send(&request(8, "daemon.ping", json!({"unexpected": true})));
    assert_eq!(error_code(&raw.recv()), -32602);

    // A notification for an unknown method gets no reply; the next call still works.
    raw.send(&json!({"jsonrpc": "2.0", "method": "nope.notify"}));
    raw.send(&request(9, "daemon.ping", Value::Null));
    let reply = raw.recv();
    assert_eq!(reply["id"], 9);
    assert_eq!(reply["result"], json!({"pong": true}));
}

#[test]
fn client_maps_unknown_methods_and_keeps_the_connection() {
    let root = common::temp_drive();
    let daemon = start(root.path(), options(), |_| {});
    let mut client = client(daemon.endpoint());
    let error = client.call("nope.missing", Value::Null).unwrap_err();
    assert_eq!(error.rpc_kind(), Some(ErrorCode::MethodNotFound));
    assert_eq!(
        client.call("daemon.ping", Value::Null).unwrap(),
        json!({"pong": true})
    );
    assert_eq!(client.server().unwrap().contract_version, 2);
}

#[test]
fn concurrent_clients_are_served_in_parallel() {
    let root = common::temp_drive();
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let entered_tx = Mutex::new(entered_tx);
    let release_rx = Mutex::new(release_rx);
    let daemon = start(root.path(), options(), move |daemon| {
        daemon
            .router_mut()
            .register("test.block", move |_, _| {
                entered_tx.lock().unwrap().send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(PATIENCE)
                    .map_err(|_| aidexd::RpcError::internal("never released"))?;
                Ok(json!("released"))
            })
            .unwrap();
    });
    let endpoint = daemon.endpoint().clone();
    let blocked = thread::spawn(move || client(&endpoint).call("test.block", Value::Null));
    entered_rx
        .recv_timeout(PATIENCE)
        .expect("blocking call started");

    let mut other = client(daemon.endpoint());
    assert_eq!(
        other.call("daemon.ping", Value::Null).unwrap(),
        json!({"pong": true})
    );
    let status = other.call("daemon.status", Value::Null).unwrap();
    assert_eq!(status["state"], "ready");

    release_tx.send(()).unwrap();
    assert_eq!(blocked.join().unwrap().unwrap(), json!("released"));
}

#[test]
fn connection_limit_rejects_extra_connections_cleanly() {
    let root = common::temp_drive();
    let mut options = options();
    options.transport.max_connections = 2;
    let daemon = start(root.path(), options, |_| {});
    let first = Raw::authenticated(daemon.endpoint());
    let _second = Raw::authenticated(daemon.endpoint());

    let mut extra = Raw::connect(daemon.endpoint());
    let reply = extra.recv();
    assert_eq!(error_code(&reply), ErrorCode::ServerBusy.code());
    extra.expect_closed();

    drop(first);
    eventually("a free connection slot", || {
        let mut retry = Raw::connect(daemon.endpoint());
        retry.send(&handshake(daemon.endpoint()));
        retry
            .recv_or_eof()
            .is_some_and(|reply| reply.get("result").is_some())
    });
}

fn register_count_stream(daemon: &mut aidexd::Daemon) {
    daemon
        .router_mut()
        .register_stream("test.count", |_, params, sink| {
            let count = params["count"].as_u64().unwrap_or(0);
            if count > 100 {
                return Err(aidexd::RpcError::invalid_params("count too large"));
            }
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
}

#[test]
fn stream_notifications_follow_the_response_in_order_and_end_cleanly() {
    let root = common::temp_drive();
    let daemon = start(root.path(), options(), register_count_stream);

    let mut raw = Raw::authenticated(daemon.endpoint());
    raw.send(&request(1, "test.count", json!({"count": 3})));
    let reply = raw.recv();
    assert_eq!(
        reply["id"], 1,
        "subscribe response must come first: {reply}"
    );
    let subscription = reply["result"]["subscription"].as_str().unwrap().to_owned();
    assert_eq!(reply["result"]["data"], json!({"count": 3}));
    for index in 0..3 {
        let notification = raw.recv();
        assert_eq!(notification["method"], "test.event");
        assert!(notification.get("id").is_none());
        assert_eq!(notification["params"]["subscription"], subscription);
        assert_eq!(notification["params"]["data"]["index"], index);
    }
    let end = raw.recv();
    assert_eq!(end["method"], "aidex.unsubscribed");
    assert_eq!(end["params"]["subscription"], subscription);

    let mut client = client(daemon.endpoint());
    let mut subscription = client.subscribe("test.count", json!({"count": 5})).unwrap();
    assert_eq!(subscription.initial(), &json!({"count": 5}));
    let received: Vec<_> = subscription
        .by_ref()
        .map(|item| item.unwrap().params["index"].as_u64().unwrap())
        .collect();
    assert_eq!(received, [0, 1, 2, 3, 4]);
    assert!(subscription.is_ended());
    assert_eq!(subscription.end_reason(), Some("completed"));
    drop(subscription);

    let error = client
        .subscribe("test.count", json!({"count": 1000}))
        .unwrap_err();
    assert_eq!(error.rpc_kind(), Some(ErrorCode::InvalidParams));
    assert_eq!(
        client.call("daemon.ping", Value::Null).unwrap()["pong"],
        true
    );
}

#[test]
fn unsubscribe_closes_the_sink_and_keeps_the_connection() {
    let root = common::temp_drive();
    let (closed_tx, closed_rx) = mpsc::channel::<SinkError>();
    let closed_tx = Mutex::new(closed_tx);
    let daemon = start(root.path(), options(), move |daemon| {
        daemon
            .router_mut()
            .register_stream("test.ticks", move |_, _, sink| {
                let report = closed_tx.lock().unwrap().clone();
                thread::spawn(move || {
                    loop {
                        if let Err(error) = sink.notify("test.event", json!("tick")) {
                            let _ = report.send(error);
                            return;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                });
                Ok(Value::Null)
            })
            .unwrap();
    });
    let mut client = client(daemon.endpoint());
    let mut subscription = client.subscribe("test.ticks", Value::Null).unwrap();
    let first = subscription.next_timeout(PATIENCE).unwrap().unwrap();
    assert_eq!(first.method, "test.event");
    assert_eq!(first.params, json!("tick"));
    assert_eq!(first.subscription.as_deref(), Some(subscription.id()));
    subscription.unsubscribe().unwrap();
    assert_eq!(closed_rx.recv_timeout(PATIENCE).unwrap(), SinkError::Closed);
    assert_eq!(
        client.call("daemon.ping", Value::Null).unwrap()["pong"],
        true
    );

    // Dropping a subscription also unsubscribes.
    let subscription = client.subscribe("test.ticks", Value::Null).unwrap();
    drop(subscription);
    assert_eq!(closed_rx.recv_timeout(PATIENCE).unwrap(), SinkError::Closed);
    assert_eq!(
        client.call("daemon.ping", Value::Null).unwrap()["pong"],
        true
    );
}

#[test]
fn slow_consumer_is_dropped_without_blocking_other_clients() {
    let root = common::temp_drive();
    let mut options = options();
    options.transport = TransportConfig {
        max_queued_messages: 16,
        max_queued_bytes: 512 * 1024,
        write_timeout: Duration::from_secs(1),
        ..options.transport
    };
    let (report_tx, report_rx) = mpsc::channel::<(u64, SinkError)>();
    let report_tx = Arc::new(Mutex::new(report_tx));
    let daemon = start(root.path(), options, move |daemon| {
        daemon
            .router_mut()
            .register_stream("test.flood", move |_, _, sink| {
                let report = report_tx.lock().unwrap().clone();
                thread::spawn(move || {
                    let padding = "x".repeat(4096);
                    for sent in 0..1_000_000u64 {
                        if let Err(error) =
                            sink.notify("test.event", json!({ "n": sent, "pad": padding }))
                        {
                            let _ = report.send((sent, error));
                            return;
                        }
                    }
                });
                Ok(Value::Null)
            })
            .unwrap();
    });

    // Subscribe, read only the subscribe response, then stop reading.
    let mut slow = Raw::authenticated(daemon.endpoint());
    slow.send(&request(1, "test.flood", Value::Null));
    assert_eq!(slow.recv()["id"], 1);

    let mut other = client(daemon.endpoint());
    let (sent, error) = report_rx
        .recv_timeout(PATIENCE)
        .expect("flood producer was never stopped");
    assert_eq!(error, SinkError::Overflow, "after {sent} notifications");
    assert_eq!(
        other.call("daemon.ping", Value::Null).unwrap()["pong"],
        true
    );
    // The slow connection is closed once its blocked write times out.
    slow.expect_closed();
    assert_eq!(
        other.call("daemon.status", Value::Null).unwrap()["state"],
        "ready"
    );
}

#[test]
fn client_reports_slow_consumer_closure() {
    let root = common::temp_drive();
    let mut options = options();
    options.transport.max_queued_messages = 4;
    let daemon = start(root.path(), options, |daemon| {
        daemon
            .router_mut()
            .register_stream("test.burst", |_, _, sink| {
                // Sends from the handler thread are held until the response is
                // queued; exceeding the queue limit drops the connection.
                for index in 0..8 {
                    let _ = sink.notify("test.event", json!(index));
                }
                Ok(Value::Null)
            })
            .unwrap();
    });
    let mut client = client(daemon.endpoint());
    // The subscribe answer may still arrive before the connection closes.
    let subscribed = client.subscribe("test.burst", Value::Null).map(drop);
    let error = match subscribed {
        Err(error) => error,
        Ok(()) => client.call("daemon.ping", Value::Null).unwrap_err(),
    };
    assert!(
        matches!(
            &error,
            ClientError::Rpc { code, .. } if *code == ErrorCode::SlowConsumer.code()
        ) || matches!(&error, ClientError::Unavailable(_)),
        "{error:?}"
    );
}
