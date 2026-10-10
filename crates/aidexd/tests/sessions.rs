//! `session.*` over authenticated IPC: ids survive a daemon restart, startup recovery
//! interrupts sessions that were running, and every transition is in the event log.

mod common;

use aidex_events::{Cursor, EventFilter};
use aidex_sessions::{LogicalState, Session, SessionState, SessionStateKind};
use aidex_workspace::Workspace;
use aidexd::{
    Client, ErrorCode,
    methods::{
        events::{REPLAY, ReplayPage},
        sessions::{CREATE, LIST, RESUME, RESUME_PLAN, ResumePlanView, SessionList},
        workspace::CREATE_DRIVE,
    },
};
use common::{options, start};
use serde_json::json;

#[test]
fn a_running_session_is_interrupted_by_restart_and_resumed_as_paused() {
    let root = common::temp_drive();
    let mut store = None;
    let daemon = start(root.path(), options(), |daemon| {
        store = Some(daemon.services().sessions().clone());
    });
    let store = store.unwrap();
    let mut client = Client::connect(root.path(), "test-sessions").unwrap();
    let workspace: Workspace = client
        .call_as(CREATE_DRIVE, json!({"name": "project"}))
        .unwrap();
    let session: Session = client
        .call_as(
            CREATE,
            json!({"workspace_id": workspace.id.as_str(), "agent_id": "aidex", "title": "feature"}),
        )
        .unwrap();
    // The harness is not wired yet; drive the session directly in-process.
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    store
        .checkpoint(
            &session.id,
            Some("phase 1"),
            LogicalState::new(json!({"step": 1})),
        )
        .unwrap();
    drop(client);
    drop(store);
    daemon.stop();
    daemon.wait().unwrap();

    let mut report = None;
    let restarted = start(root.path(), options(), |daemon| {
        report = Some(daemon.services().session_recovery().clone());
    });
    let report = report.unwrap();
    assert_eq!(report.interrupted.len(), 1);
    assert_eq!(report.interrupted[0].session_id, session.id);
    assert!(report.interrupted[0].recoverable);
    let mut client = Client::connect(root.path(), "test-sessions").unwrap();
    let list: SessionList = client.call_as(LIST, json!({})).unwrap();
    assert_eq!(list.sessions.len(), 1);
    assert_eq!(
        list.sessions[0].id, session.id,
        "the id is stable across restarts"
    );
    assert_eq!(list.sessions[0].kind(), SessionStateKind::Interrupted);

    let plan: ResumePlanView = client
        .call_as(RESUME_PLAN, json!({"id": session.id.as_str()}))
        .unwrap();
    assert!(plan.resumable);
    assert!(!plan.adopted);
    assert_eq!(plan.checkpoint.unwrap().label.as_deref(), Some("phase 1"));
    let resumed: ResumePlanView = client
        .call_as(RESUME, json!({"id": session.id.as_str()}))
        .unwrap();
    assert!(resumed.adopted);
    assert!(!resumed.continuation_available);
    assert_eq!(resumed.session.state, SessionState::Paused);
    let again = client
        .call(RESUME, json!({"id": "ses_missing"}))
        .unwrap_err();
    assert_eq!(again.rpc_kind(), Some(ErrorCode::NotFound));

    let page: ReplayPage = client
        .call_as(
            REPLAY,
            json!({"after": Cursor::ZERO, "filter": EventFilter::for_session(session.id.clone())}),
        )
        .unwrap();
    let names: Vec<_> = page.events.iter().map(|event| event.name()).collect();
    assert_eq!(
        names,
        [
            "session.created",
            "session.started",
            "session.checkpointed",
            "session.interrupted",
            "session.paused"
        ]
    );
    restarted.stop();
    restarted.wait().unwrap();
}
