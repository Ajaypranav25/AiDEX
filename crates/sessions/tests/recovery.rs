//! Startup recovery: crash/restart, lost processes, cancelled approvals, and explicit
//! gaps for interrupted or failed event appends — never fabricated events.

mod common;

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use aidex_events::{Cursor, EventFilter, EventLog, RetentionPolicy};
use aidex_sessions::{
    ApprovalRef, ApprovalScope, ApprovalStatus, EventGapKind, LogicalState, NewToolCall,
    RESTART_REASON, SessionState, SessionStateKind, ToolCallState, ToolCallUpdate,
};
use common::{
    CrashingSink, FailingSink, id, laptop, new_session, open, open_with_sink, paths,
    session_events, temp_drive, workstation,
};
use serde_json::json;

#[test]
fn crash_restart_marks_active_sessions_interrupted_with_correct_recoverability() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let checkpointed = new_session(&store, "has checkpoint");
    store
        .transition(&checkpointed.id, SessionState::Running)
        .unwrap();
    store
        .checkpoint(
            &checkpointed.id,
            Some("p1"),
            LogicalState::new(json!({"step": 3})),
        )
        .unwrap();
    let bare = new_session(&store, "no checkpoint");
    store.transition(&bare.id, SessionState::Running).unwrap();
    store
        .transition(&bare.id, SessionState::WaitingApproval)
        .unwrap();
    store
        .record_approval_ref(
            &bare.id,
            ApprovalRef {
                approval_id: id("apr_wait"),
                intent_fingerprint_hex: "0f".repeat(32),
                decision: ApprovalStatus::Pending,
                scope: ApprovalScope::Once,
                expires_at: None,
            },
        )
        .unwrap();
    let paused = new_session(&store, "paused");
    store.transition(&paused.id, SessionState::Running).unwrap();
    store.transition(&paused.id, SessionState::Paused).unwrap();
    store
        .record_tool_call(
            &checkpointed.id,
            NewToolCall {
                state: ToolCallState::Running,
                process_id: Some(id("proc_tests")),
                ..NewToolCall::new(
                    id("call_test"),
                    id("shell.exec"),
                    json!({"command": "cargo test"}),
                )
            },
        )
        .unwrap();
    // Crash: the store is dropped without any clean shutdown step.
    drop(store);

    let (store, sink) = open(root.path(), laptop());
    let report = store.recover_on_startup(&laptop(), None).unwrap();
    assert_eq!(report.interrupted.len(), 2);
    let first = &report.interrupted[0];
    assert_eq!(first.session_id, checkpointed.id);
    assert_eq!(first.previous_state, SessionStateKind::Running);
    assert!(first.recoverable);
    assert!(first.checkpoint_id.is_some());
    assert!(!first.different_host);
    let second = &report.interrupted[1];
    assert_eq!(second.session_id, bare.id);
    assert_eq!(second.previous_state, SessionStateKind::WaitingApproval);
    assert!(!second.recoverable);
    assert_eq!(
        store.get(&checkpointed.id).unwrap().state,
        SessionState::Interrupted {
            reason: RESTART_REASON.into(),
            recoverable: true
        }
    );
    assert_eq!(
        store.get(&bare.id).unwrap().state,
        SessionState::Interrupted {
            reason: RESTART_REASON.into(),
            recoverable: false
        }
    );
    assert_eq!(store.get(&paused.id).unwrap().state, SessionState::Paused);
    assert_eq!(
        session_events(&sink, &checkpointed.id),
        ["session.interrupted"]
    );
    assert!(session_events(&sink, &paused.id).is_empty());

    // Host-bound process state is marked lost, never "resumed".
    assert_eq!(report.lost_tool_calls.len(), 1);
    assert_eq!(report.lost_tool_calls[0].process_id, Some(id("proc_tests")));
    let call = store.tool_call(&checkpointed.id, &id("call_test")).unwrap();
    assert_eq!(call.state, ToolCallState::Lost);
    assert!(call.finished_at.is_some());
    assert!(call.result_summary.unwrap().contains("not restored"));
    assert_eq!(report.cancelled_approvals.len(), 1);
    let approvals = store.approvals(&bare.id).unwrap();
    assert_eq!(approvals[0].approval.decision, ApprovalStatus::Cancelled);

    // Recovery is idempotent.
    let again = store.recover_on_startup(&laptop(), None).unwrap();
    assert!(again.is_clean(), "{again:?}");
}

#[test]
fn a_corrupt_only_checkpoint_makes_the_session_unrecoverable() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "corrupt only");
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    let meta = store
        .checkpoint(&session.id, None, LogicalState::new(json!({})))
        .unwrap();
    let blob = store
        .database_path()
        .parent()
        .unwrap()
        .join(aidex_sessions::BLOB_DIR)
        .join("artifacts")
        .join(&meta.sha256);
    drop(store);
    std::fs::write(&blob, b"{\"format\":1}").unwrap();
    let (store, _) = open(root.path(), laptop());
    let report = store.recover_on_startup(&laptop(), None).unwrap();
    assert!(!report.interrupted[0].recoverable);
    assert_eq!(report.corrupt_checkpoints, [meta.id]);
}

#[test]
fn a_crash_during_event_append_is_recorded_as_a_gap_not_replayed() {
    let root = temp_drive();
    let sink = Arc::new(CrashingSink::default());
    let store = open_with_sink(root.path(), laptop(), sink.clone());
    let session = new_session(&store, "crash mid append");
    sink.arm();
    let crashed = catch_unwind(AssertUnwindSafe(|| {
        store.transition(&session.id, SessionState::Running)
    }));
    assert!(crashed.is_err(), "the simulated crash must abort the call");
    drop(store);
    assert_eq!(
        sink.names(),
        ["session.created"],
        "session.started never reached the log"
    );

    let (store, recovered_sink) = open(root.path(), laptop());
    // The state change itself committed before the crash.
    let report = store.recover_on_startup(&laptop(), None).unwrap();
    let gap = &report.event_gaps[0];
    assert_eq!(gap.kind, EventGapKind::Interrupted);
    assert_eq!(gap.event.as_deref(), Some("session.started"));
    assert_eq!(gap.session_id, session.id);
    assert_eq!(store.event_gaps(&session.id).unwrap(), report.event_gaps);
    // The session was running, so it is interrupted now; the lost `session.started`
    // is not re-emitted.
    assert_eq!(
        session_events(&recovered_sink, &session.id),
        ["session.interrupted"]
    );
    assert_eq!(
        store.get(&session.id).unwrap().kind(),
        SessionStateKind::Interrupted
    );
}

#[test]
fn a_failed_event_append_keeps_the_change_and_records_the_gap() {
    let root = temp_drive();
    let store = open_with_sink(root.path(), laptop(), Arc::new(FailingSink));
    let session = new_session(&store, "no events");
    let running = store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    assert_eq!(running.state, SessionState::Running);
    assert_eq!(running.last_event_cursor, Cursor::ZERO);
    let gaps = store.event_gaps(&session.id).unwrap();
    assert_eq!(
        gaps.iter()
            .map(|gap| (gap.kind, gap.event.as_deref()))
            .collect::<Vec<_>>(),
        [
            (EventGapKind::EmitFailed, Some("session.created")),
            (EventGapKind::EmitFailed, Some("session.started"))
        ]
    );
    assert!(gaps[0].detail.contains("disk I/O error"));
    let summary = store.summary(&session.id).unwrap();
    assert_eq!(summary.event_gaps.len(), 2);
}

#[test]
fn a_session_cursor_ahead_of_the_event_log_is_an_explicit_gap() {
    let root = temp_drive();
    let drive = paths(root.path());
    let events_dir = drive.ensure_dir(&drive.sessions()).unwrap();
    let log = Arc::new(
        EventLog::open(
            events_dir.join("events.sqlite3"),
            RetentionPolicy::UNBOUNDED,
        )
        .unwrap(),
    );
    let store = open_with_sink(root.path(), laptop(), log.clone());
    let session = new_session(&store, "log replaced");
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    let recorded = store.get(&session.id).unwrap().last_event_cursor;
    assert_eq!(recorded, Cursor::new(2));
    assert_eq!(log.latest_cursor().unwrap(), recorded);
    drop(store);
    drop(log);

    // The event log is replaced (restored from an older backup / lost its file).
    std::fs::remove_file(events_dir.join("events.sqlite3")).unwrap();
    for sidecar in ["events.sqlite3-wal", "events.sqlite3-shm"] {
        let _ = std::fs::remove_file(events_dir.join(sidecar));
    }
    let replaced = Arc::new(
        EventLog::open(
            events_dir.join("events.sqlite3"),
            RetentionPolicy::UNBOUNDED,
        )
        .unwrap(),
    );
    let store = open_with_sink(root.path(), laptop(), replaced.clone());
    let report = store
        .recover_on_startup(&laptop(), Some(replaced.latest_cursor().unwrap()))
        .unwrap();
    let gap = report
        .event_gaps
        .iter()
        .find(|gap| gap.kind == EventGapKind::LogBehind)
        .unwrap();
    assert_eq!(gap.session_cursor, Some(Cursor::new(2)));
    assert_eq!(gap.log_cursor, Some(Cursor::ZERO));
    // No history was invented: the new log holds only what recovery itself did.
    let page = replaced
        .replay(&EventFilter::all(), Cursor::ZERO, 100)
        .unwrap();
    let names: Vec<_> = page.events.iter().map(|event| event.name()).collect();
    assert_eq!(names, ["session.interrupted"]);
    assert_eq!(
        store.get(&session.id).unwrap().last_event_cursor,
        Cursor::new(1)
    );
}

#[test]
fn recovery_reports_sessions_that_last_ran_on_another_host() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "moved");
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    store
        .record_tool_call(
            &session.id,
            NewToolCall::new(id("call_1"), id("fs.read"), json!({})),
        )
        .unwrap();
    store
        .update_tool_call(
            &session.id,
            &id("call_1"),
            ToolCallUpdate::state(ToolCallState::Running),
        )
        .unwrap();
    drop(store);
    let (store, _) = open(root.path(), workstation());
    let report = store.recover_on_startup(&workstation(), None).unwrap();
    assert!(report.interrupted[0].different_host);
    assert_eq!(report.lost_tool_calls[0].process_id, None);
}
