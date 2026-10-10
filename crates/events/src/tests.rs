use std::{thread, time::Duration};

use super::{EventError, EventKind, EventLog};

fn task_started(id: &str) -> EventKind {
    EventKind::TaskStarted { task_id: id.into() }
}

#[test]
fn assigns_ordered_sequences_and_replays_from_acknowledged_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::open(dir.path().join("events.sqlite"), 20).unwrap();
    for id in ["a", "b", "c"] {
        log.publish("workspace", Some("session".into()), task_started(id))
            .unwrap();
    }
    let replay = log.read_after("workspace", 1, 10).unwrap();
    assert_eq!(
        replay
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(replay[0].session_id.as_deref(), Some("session"));
    assert_eq!(log.read_after("other", 0, 10).unwrap(), []);
}

#[test]
fn persisted_cursor_survives_reopening_the_daemon_log() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.sqlite");
    let first = EventLog::open(&path, 20).unwrap();
    first
        .publish("workspace", None, task_started("one"))
        .unwrap();
    drop(first);
    let restarted = EventLog::open(&path, 20).unwrap();
    assert_eq!(
        restarted
            .publish("workspace", None, task_started("two"))
            .unwrap()
            .sequence,
        2
    );
    assert_eq!(restarted.read_after("workspace", 0, 10).unwrap().len(), 2);
}

#[test]
fn retention_reports_expired_cursors_instead_of_silently_skipping_events() {
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::open(dir.path().join("events.sqlite"), 2).unwrap();
    for id in ["a", "b", "c"] {
        log.publish("workspace", None, task_started(id)).unwrap();
    }
    assert_eq!(
        log.read_after("workspace", 0, 10),
        Err(EventError::CursorExpired {
            requested: 0,
            earliest: 2
        })
    );
    assert_eq!(
        log.read_after("workspace", 1, 10)
            .unwrap()
            .iter()
            .map(|e| e.sequence)
            .collect::<Vec<_>>(),
        [2, 3]
    );
}

#[test]
fn full_live_queue_does_not_block_publisher_and_replay_recovers_after_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::open(dir.path().join("events.sqlite"), 20).unwrap();
    let subscription = log.subscribe("workspace", 1).unwrap();
    log.publish("workspace", None, task_started("one")).unwrap();
    let log_for_publisher = &log;
    thread::scope(|scope| {
        scope.spawn(|| {
            log_for_publisher
                .publish("workspace", None, task_started("two"))
                .unwrap()
        });
    });
    assert_eq!(subscription.recv().unwrap().sequence, 1);
    assert_eq!(
        subscription
            .recv_timeout(Duration::from_millis(100))
            .unwrap_err(),
        std::sync::mpsc::RecvTimeoutError::Disconnected
    );
    assert_eq!(log.read_after("workspace", 1, 10).unwrap()[0].sequence, 2);
}

#[test]
fn event_wire_shape_is_versioned_and_contains_only_declared_transition_fields() {
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::open(dir.path().join("events.sqlite"), 4).unwrap();
    let event = log
        .publish(
            "workspace",
            None,
            EventKind::ApprovalRequested {
                approval_id: "approval-1".into(),
            },
        )
        .unwrap();
    let json = serde_json::to_value(event).unwrap();
    assert_eq!(json["version"], 1);
    assert_eq!(json["workspace_id"], "workspace");
    assert_eq!(json["kind"]["type"], "approval_requested");
    assert!(json.get("token").is_none());
}
#[test]
fn rejects_payload_fields_that_cannot_be_safe_identifiers() {
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::open(dir.path().join("events.sqlite"), 4).unwrap();
    assert_eq!(
        log.publish("workspace", None, task_started("secret value with spaces")),
        Err(EventError::InvalidIdentifier)
    );
    assert_eq!(log.read_after("workspace", 0, 10).unwrap(), []);
}

#[test]
fn rejects_unbounded_batches_and_unrepresentable_cursors() {
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::open(dir.path().join("events.sqlite"), 4).unwrap();
    assert_eq!(
        log.read_after("workspace", 0, 0),
        Err(EventError::InvalidLimit)
    );
    assert_eq!(
        log.read_after("workspace", 0, super::MAX_BATCH + 1),
        Err(EventError::InvalidLimit)
    );
    assert_eq!(
        log.read_after("workspace", i64::MAX as u64 + 1, 1),
        Err(EventError::InvalidCursor)
    );
    assert_eq!(
        log.read_after("../outside", 0, 1),
        Err(EventError::InvalidIdentifier)
    );
    assert_eq!(
        log.subscribe("workspace", super::MAX_BATCH + 1).err(),
        Some(EventError::InvalidLimit)
    );
}
