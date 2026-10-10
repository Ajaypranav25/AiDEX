//! Resume plans across hosts, and adopting a session for resumption.

mod common;

use aidex_core::ModelReference;
use aidex_sessions::{
    LogicalState, MessageRole, NewMessage, NewToolCall, SessionError, SessionState,
    SessionStateKind, ToolCallState, UnrecoverableKind,
};
use common::{id, laptop, new_session, open, session_events, temp_drive, workstation};
use serde_json::json;

/// A session that ran on the laptop, checkpointed, ran a process, and was interrupted.
fn interrupted_on_laptop(root: &std::path::Path) -> aidex_core::EntityId {
    let (store, _) = open(root, laptop());
    let session = new_session(&store, "move me");
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    store
        .set_model(
            &session.id,
            Some(ModelReference {
                provider_id: id("local"),
                model_id: id("gemma-4-e2b"),
            }),
        )
        .unwrap();
    store
        .append_message(
            &session.id,
            NewMessage::new(MessageRole::User, "implement it"),
        )
        .unwrap();
    store
        .checkpoint(
            &session.id,
            Some("implemented"),
            LogicalState::new(json!({"done": ["a"]})),
        )
        .unwrap();
    store
        .append_message(
            &session.id,
            NewMessage::new(MessageRole::Assistant, "running tests"),
        )
        .unwrap();
    store
        .record_tool_call(
            &session.id,
            NewToolCall {
                state: ToolCallState::Running,
                process_id: Some(id("proc_cargo")),
                ..NewToolCall::new(
                    id("call_tests"),
                    id("shell.exec"),
                    json!({"command": "cargo test"}),
                )
            },
        )
        .unwrap();
    drop(store);
    // Unplugged mid-task; the next daemon (on the laptop) recovers it.
    let (store, _) = open(root, laptop());
    store.recover_on_startup(&laptop(), None).unwrap();
    session.id
}

#[test]
fn a_different_host_is_flagged_and_lost_processes_are_unrecoverable() {
    let root = temp_drive();
    let session = interrupted_on_laptop(root.path());
    let (store, _) = open(root.path(), workstation());
    let plan = store.resume_plan(&session, store.host()).unwrap();
    assert!(plan.resumable, "{:?}", plan.blocked_reason);
    assert!(plan.different_host);
    assert_eq!(
        plan.host_changes,
        ["operating system changed from windows to linux"]
    );
    let checkpoint = plan.checkpoint.as_ref().unwrap();
    assert_eq!(checkpoint.state.harness, json!({"done": ["a"]}));
    assert_eq!(checkpoint.host_fingerprint, laptop());
    assert_eq!(
        plan.messages_since_checkpoint
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        ["running tests"]
    );
    let kinds: Vec<_> = plan
        .unrecoverable
        .iter()
        .map(|item| {
            (
                item.kind,
                item.subject.as_ref().map(|s| s.as_str().to_owned()),
            )
        })
        .collect();
    assert!(kinds.contains(&(UnrecoverableKind::Process, Some("proc_cargo".into()))));
    assert!(kinds.contains(&(UnrecoverableKind::ToolCall, Some("call_tests".into()))));
    assert!(kinds.contains(&(UnrecoverableKind::ModelState, None)));
    assert!(
        kinds.contains(&(UnrecoverableKind::ModelState, Some("gemma-4-e2b".into()))),
        "a model chosen on another host must be re-confirmed"
    );
}

#[test]
fn the_same_host_is_not_flagged() {
    let root = temp_drive();
    let session = interrupted_on_laptop(root.path());
    let (store, _) = open(root.path(), laptop());
    let plan = store.resume_plan(&session, &laptop()).unwrap();
    assert!(!plan.different_host);
    assert!(plan.host_changes.is_empty());
    assert!(
        !plan
            .unrecoverable
            .iter()
            .any(|item| item.kind == UnrecoverableKind::ModelState && item.subject.is_some())
    );
}

#[test]
fn mark_resumed_adopts_the_session_without_starting_it() {
    let root = temp_drive();
    let session = interrupted_on_laptop(root.path());
    let (store, sink) = open(root.path(), workstation());
    let adopted = store.mark_resumed(&session).unwrap();
    assert_eq!(adopted.state, SessionState::Paused);
    assert_eq!(adopted.host_fingerprint, workstation());
    assert!(adopted.resumed_at.is_some());
    assert_eq!(session_events(&sink, &session), ["session.paused"]);
    // Adopted: the plan now sees this host as the session's host.
    assert!(
        !store
            .resume_plan(&session, &workstation())
            .unwrap()
            .different_host
    );
    // Marking again keeps it paused; the harness then continues it.
    assert_eq!(
        store.mark_resumed(&session).unwrap().kind(),
        SessionStateKind::Paused
    );
    store.transition(&session, SessionState::Running).unwrap();
    assert_eq!(
        session_events(&sink, &session),
        ["session.paused", "session.resumed"]
    );
}

#[test]
fn finished_or_active_sessions_are_not_resumable() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "active");
    let created = store.resume_plan(&session.id, &laptop()).unwrap();
    assert!(!created.resumable);
    assert!(created.checkpoint.is_none());
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    let running = store.resume_plan(&session.id, &laptop()).unwrap();
    assert!(!running.resumable);
    assert!(running.blocked_reason.unwrap().contains("still active"));
    let error = store.mark_resumed(&session.id).unwrap_err();
    assert!(matches!(error, SessionError::InvalidTransition { .. }));
    store
        .transition(&session.id, SessionState::Completed)
        .unwrap();
    let done = store.resume_plan(&session.id, &laptop()).unwrap();
    assert_eq!(
        done.blocked_reason.as_deref(),
        Some("the session is completed")
    );
    assert!(store.mark_resumed(&session.id).is_err());
}

#[test]
fn a_plan_without_a_valid_checkpoint_resumes_from_messages_only() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "messages only");
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    store
        .append_message(&session.id, NewMessage::new(MessageRole::User, "hello"))
        .unwrap();
    let interrupted = store.interrupt(&session.id, "drive removed").unwrap();
    assert_eq!(
        interrupted.state,
        SessionState::Interrupted {
            reason: "drive removed".into(),
            recoverable: false
        }
    );
    let plan = store.resume_plan(&session.id, &laptop()).unwrap();
    assert!(plan.resumable);
    assert!(plan.checkpoint.is_none());
    assert_eq!(plan.messages_since_checkpoint.len(), 1);
}
