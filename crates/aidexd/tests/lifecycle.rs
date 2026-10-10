//! Lifecycle: stop, drive removal, single instance, and stale-endpoint recovery.

mod common;

use aidexd::{
    Client, ClientError, Daemon, DaemonError, EndpointError, Recovery, SinkError, StopReason,
    endpoint, lifecycle,
};
use common::{PATIENCE, Raw, client, options, start};
use serde_json::{Value, json};
use std::{
    fs,
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

fn write_endpoint(root: &std::path::Path, contents: &str) {
    fs::create_dir_all(endpoint::state_dir(root)).unwrap();
    fs::write(endpoint::state_path(root), contents).unwrap();
}

fn unused_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

#[test]
fn daemon_stop_notifies_subscribers_closes_connections_and_removes_endpoint() {
    let root = tempfile::tempdir().unwrap();
    let (closed_tx, closed_rx) = mpsc::channel::<SinkError>();
    let closed_tx = Mutex::new(closed_tx);
    let daemon = start(root.path(), options(), move |daemon| {
        daemon
            .router_mut()
            .register_stream("test.idle", move |_, _, sink| {
                let report = closed_tx.lock().unwrap().clone();
                thread::spawn(move || {
                    while !sink.is_closed() {
                        thread::sleep(Duration::from_millis(5));
                    }
                    let _ = report.send(
                        sink.notify("test.event", Value::Null)
                            .expect_err("closed sink must refuse"),
                    );
                });
                Ok(Value::Null)
            })
            .unwrap();
    });
    assert!(endpoint::state_path(root.path()).is_file());

    let mut subscriber = Raw::authenticated(daemon.endpoint());
    subscriber.send(&common::request(1, "test.idle", Value::Null));
    assert_eq!(subscriber.recv()["id"], 1);
    let mut idle = Raw::authenticated(daemon.endpoint());

    let mut controller = Client::connect(root.path(), "test-controller").unwrap();
    assert_eq!(
        controller.call("daemon.stop", Value::Null).unwrap(),
        json!({"state": "stopping"})
    );
    for raw in [&mut subscriber, &mut idle] {
        let closing = raw.recv();
        assert_eq!(closing["method"], "aidex.closing", "{closing}");
        assert_eq!(closing["params"]["code"], -32014);
        raw.expect_closed();
    }
    assert_eq!(closed_rx.recv_timeout(PATIENCE).unwrap(), SinkError::Closed);
    assert_eq!(daemon.wait().unwrap(), StopReason::Requested);
    assert!(!endpoint::state_path(root.path()).exists());
    assert!(matches!(
        controller.call("daemon.ping", Value::Null),
        Err(ClientError::Unavailable(_))
    ));
    assert!(matches!(
        Client::connect(root.path(), "late"),
        Err(ClientError::Unavailable(_))
    ));
}

#[test]
fn removed_drive_stops_the_daemon_and_closes_connections() {
    let root = tempfile::tempdir().unwrap();
    let sentinel = root.path().join("user-data.txt");
    fs::write(&sentinel, b"preserve").unwrap();
    let removed = Arc::new(AtomicBool::new(false));
    let mut options = options();
    let probe = Arc::clone(&removed);
    options.drive_present = Arc::new(move |_| !probe.load(Ordering::SeqCst));
    let daemon = start(root.path(), options, |_| {});
    let mut connection = Raw::authenticated(daemon.endpoint());

    removed.store(true, Ordering::SeqCst);
    let closing = connection.recv();
    assert_eq!(closing["method"], "aidex.closing");
    assert!(
        closing["params"]["message"]
            .as_str()
            .unwrap()
            .contains("drive was removed")
    );
    connection.expect_closed();
    assert_eq!(daemon.wait().unwrap(), StopReason::DriveRemoved);
    assert_eq!(fs::read(sentinel).unwrap(), b"preserve");
}

/// A real rename: on Windows the open event database prevents renaming the
/// directory, which the probe-based test above covers instead.
#[cfg(not(windows))]
#[test]
fn daemon_exits_after_the_drive_directory_disappears() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("drive");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("user-data.txt"), b"preserve").unwrap();
    let daemon = start(&root, options(), |_| {});
    client(daemon.endpoint())
        .call("daemon.ping", Value::Null)
        .unwrap();
    let moved = parent.path().join("drive-removed");
    fs::rename(&root, &moved).unwrap();
    assert_eq!(daemon.wait().unwrap(), StopReason::DriveRemoved);
    assert_eq!(fs::read(moved.join("user-data.txt")).unwrap(), b"preserve");
}

#[test]
fn unavailable_drive_is_not_recreated() {
    let parent = tempfile::tempdir().unwrap();
    let missing = parent.path().join("missing");
    assert!(matches!(
        Daemon::new(&missing),
        Err(DaemonError::DriveUnavailable(_))
    ));
    assert!(!missing.exists());
}

#[test]
fn a_second_daemon_cannot_replace_a_live_endpoint() {
    let root = tempfile::tempdir().unwrap();
    let first = start(root.path(), options(), |_| {});
    let second = Daemon::with_options(root.path(), options())
        .unwrap()
        .start();
    assert!(matches!(
        second,
        Err(DaemonError::Endpoint(EndpointError::Exists(_)))
    ));
    assert!(matches!(
        lifecycle::recover_stale_endpoint(root.path()),
        Err(DaemonError::AlreadyRunning { pid }) if pid == std::process::id()
    ));
    client(first.endpoint())
        .call("daemon.ping", Value::Null)
        .unwrap();
    first.stop();
    first.wait().unwrap();

    // Restart on the same drive after a clean stop.
    let restarted = start(root.path(), options(), |_| {});
    let status = Client::connect(root.path(), "test")
        .unwrap()
        .call("daemon.status", Value::Null)
        .unwrap();
    assert_eq!(status["state"], "ready");
    assert_eq!(status["contract_version"], 2);
    restarted.stop();
    restarted.wait().unwrap();
}

#[test]
fn stale_endpoint_recovery_removes_only_unreachable_daemon_metadata() {
    let root = tempfile::tempdir().unwrap();
    let sentinel = root.path().join("user-data.txt");
    fs::write(&sentinel, b"preserve").unwrap();
    assert_eq!(
        lifecycle::recover_stale_endpoint(root.path()).unwrap(),
        Recovery::NoEndpoint
    );

    // A crashed v2 daemon: valid file, nothing listening.
    let daemon = start(root.path(), options(), |_| {});
    let published = fs::read(endpoint::state_path(root.path())).unwrap();
    daemon.stop();
    daemon.wait().unwrap();
    fs::write(endpoint::state_path(root.path()), &published).unwrap();
    assert!(matches!(
        Client::connect(root.path(), "test"),
        Err(ClientError::Unavailable(_))
    ));
    assert_eq!(
        lifecycle::recover_stale_endpoint(root.path()).unwrap(),
        Recovery::RemovedStale
    );
    assert!(!endpoint::state_path(root.path()).exists());

    // A crashed v1 daemon is recoverable once unreachable.
    write_endpoint(
        root.path(),
        &format!(
            r#"{{"version":1,"pid":1,"port":{},"token":"opaque"}}"#,
            unused_port()
        ),
    );
    assert_eq!(
        lifecycle::recover_stale_endpoint(root.path()).unwrap(),
        Recovery::RemovedStale
    );
    assert_eq!(fs::read(&sentinel).unwrap(), b"preserve");
    assert!(endpoint::state_dir(root.path()).is_dir());
}

#[test]
fn reachable_or_unreadable_endpoints_are_never_removed() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    write_endpoint(
        root.path(),
        &format!(r#"{{"version":1,"pid":1,"port":{port},"token":"opaque"}}"#),
    );
    assert!(matches!(
        Client::connect(root.path(), "test"),
        Err(ClientError::Incompatible { .. })
    ));
    let error = lifecycle::recover_stale_endpoint(root.path()).unwrap_err();
    assert!(matches!(error, DaemonError::EndpointBusy(_)), "{error}");
    assert!(error.to_string().contains("contract-v1"));
    assert!(endpoint::state_path(root.path()).exists());
    drop(listener);

    write_endpoint(root.path(), "{corrupt");
    assert!(matches!(
        lifecycle::recover_stale_endpoint(root.path()),
        Err(DaemonError::EndpointBusy(_))
    ));
    assert!(endpoint::state_path(root.path()).exists());
}

#[cfg(unix)]
#[test]
fn symlinked_runtime_directories_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.path().join(".aidex")).unwrap();
    assert!(matches!(
        Daemon::new(root.path()),
        Err(DaemonError::Services(_))
    ));
}
