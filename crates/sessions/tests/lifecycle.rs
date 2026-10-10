//! Session lifecycle: creation, the state machine and its events, listing, messages,
//! and identity across a store reopen (daemon restart) and a drive move.

mod common;

use aidex_core::ModelReference;
use aidex_events::{Cursor, EventScope};
use aidex_sessions::{
    BudgetUsage, LogicalState, MessageRole, NewMessage, NewSession, SessionError, SessionFilter,
    SessionState, SessionStateKind,
};
use common::{copy_dir, id, laptop, new_session, open, session_events, temp_drive};
use serde_json::json;

#[test]
fn ids_and_history_survive_a_restart_and_a_drive_move() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "implement feature");
    assert!(session.id.as_str().starts_with("ses_"));
    store
        .append_message(
            &session.id,
            NewMessage::new(MessageRole::User, "add a flag"),
        )
        .unwrap();
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    let checkpoint = store
        .checkpoint(
            &session.id,
            Some("phase 1"),
            LogicalState::new(json!({"step": 1})),
        )
        .unwrap();
    store.transition(&session.id, SessionState::Paused).unwrap();
    drop(store);

    // Daemon restart: same ids, state, messages, and checkpoint.
    let (store, _) = open(root.path(), laptop());
    let reopened = store.get(&session.id).unwrap();
    assert_eq!(reopened.id, session.id);
    assert_eq!(reopened.state, SessionState::Paused);
    assert_eq!(reopened.created_at, session.created_at);
    assert_eq!(
        store.messages(&session.id, 0, 10).unwrap().messages.len(),
        1
    );
    drop(store);

    // Drive move: copy the whole drive to another path (another drive letter).
    let moved = tempfile::tempdir().unwrap();
    copy_dir(root.path(), moved.path());
    drop(root);
    let (store, _) = open(moved.path(), laptop());
    let listed = store.list(&SessionFilter::all()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, session.id);
    let restored = store.latest_valid_checkpoint(&session.id).unwrap().unwrap();
    assert_eq!(restored.checkpoint.meta.id, checkpoint.id);
    assert!(!restored.recovered_from_corruption);
    assert_eq!(restored.checkpoint.state.harness, json!({"step": 1}));
    let page = store.messages(&session.id, 0, 10).unwrap();
    assert_eq!(page.messages[0].content, "add a flag");
}

#[test]
fn records_never_contain_absolute_host_paths() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "portable");
    store
        .checkpoint(&session.id, None, LogicalState::new(json!({})))
        .unwrap();
    let database = store.database_path().to_path_buf();
    drop(store);
    let bytes = common::all_bytes(database.parent().unwrap());
    let root_text = root.path().display().to_string();
    assert!(!common::contains(&bytes, &root_text), "drive root leaked");
}

#[test]
fn every_state_change_emits_its_event_in_the_session_scope() {
    let root = temp_drive();
    let (store, sink) = open(root.path(), laptop());
    let session = new_session(&store, "events");
    for state in [
        SessionState::Running,
        SessionState::WaitingApproval,
        SessionState::Running,
        SessionState::Paused,
        SessionState::Running,
        SessionState::Completed,
    ] {
        store.transition(&session.id, state).unwrap();
    }
    assert_eq!(
        session_events(&sink, &session.id),
        [
            "session.created",
            "session.started",
            "session.paused",
            "session.resumed",
            "session.completed"
        ]
    );
    let created = &sink.events()[0];
    assert_eq!(
        created.scope,
        EventScope::session(id("wsp_main"), session.id.clone())
    );
    let stored = store.get(&session.id).unwrap();
    assert_eq!(stored.last_event_cursor, Cursor::new(5));
    assert!(store.event_gaps(&session.id).unwrap().is_empty());

    let failing = new_session(&store, "fails");
    store
        .transition(&failing.id, SessionState::Running)
        .unwrap();
    let failed = store
        .transition(
            &failing.id,
            SessionState::Failed {
                code: "provider.unavailable".into(),
                message: "model endpoint refused the request".into(),
            },
        )
        .unwrap();
    assert!(failed.state.is_terminal());
    let names = session_events(&sink, &failing.id);
    assert_eq!(names.last(), Some(&"session.failed"));
}

#[test]
fn invalid_transitions_are_rejected_and_change_nothing() {
    let root = temp_drive();
    let (store, sink) = open(root.path(), laptop());
    let session = new_session(&store, "machine");
    for (from_setup, target) in [
        (vec![], SessionState::Completed),
        (vec![], SessionState::Paused),
        (vec![], SessionState::Created),
        (vec![SessionState::Running], SessionState::Running),
    ] {
        for state in from_setup {
            store.transition(&session.id, state).unwrap();
        }
        let error = store.transition(&session.id, target.clone()).unwrap_err();
        assert!(
            matches!(error, SessionError::InvalidTransition { .. }),
            "{target:?}: {error}"
        );
        assert_eq!(error.reason(), "invalid_transition");
    }
    store
        .transition(&session.id, SessionState::Cancelled)
        .unwrap();
    let before = sink.len();
    for state in [
        SessionState::Running,
        SessionState::Paused,
        SessionState::Cancelled,
    ] {
        let error = store.transition(&session.id, state).unwrap_err();
        assert!(matches!(
            error,
            SessionError::InvalidTransition {
                from: SessionStateKind::Cancelled,
                ..
            }
        ));
    }
    assert_eq!(sink.len(), before, "rejected transitions emit nothing");
    assert_eq!(
        store.get(&session.id).unwrap().state,
        SessionState::Cancelled
    );
    let closed = store
        .append_message(&session.id, NewMessage::new(MessageRole::User, "late"))
        .unwrap_err();
    assert!(matches!(closed, SessionError::SessionClosed { .. }));
    let bad_code = store
        .transition(
            &new_session(&store, "x").id,
            SessionState::Failed {
                code: "Not A Code".into(),
                message: String::new(),
            },
        )
        .unwrap_err();
    assert!(matches!(bad_code, SessionError::InvalidArgument(_)));
}

#[test]
fn the_transition_table_matches_the_documented_machine() {
    use SessionStateKind::*;
    let allowed = |from: SessionStateKind| -> Vec<SessionStateKind> {
        SessionStateKind::ALL
            .into_iter()
            .filter(|to| from.can_transition_to(*to))
            .collect()
    };
    assert_eq!(allowed(Created), [Running, Failed, Cancelled]);
    assert_eq!(
        allowed(Running),
        [
            WaitingApproval,
            Paused,
            Completed,
            Failed,
            Cancelled,
            Interrupted
        ]
    );
    assert_eq!(
        allowed(WaitingApproval),
        [Running, Paused, Failed, Cancelled, Interrupted]
    );
    assert_eq!(allowed(Paused), [Running, Failed, Cancelled]);
    assert_eq!(allowed(Interrupted), [Running, Paused, Failed, Cancelled]);
    for terminal in [Completed, Failed, Cancelled] {
        assert!(allowed(terminal).is_empty());
        assert!(terminal.is_terminal());
    }
}

#[test]
fn list_filters_by_workspace_and_state_newest_first() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let first = new_session(&store, "first");
    let second = new_session(&store, "second");
    let other = store
        .create(NewSession::new(
            id("wsp_other"),
            id("agent_native"),
            "other",
        ))
        .unwrap();
    store.transition(&second.id, SessionState::Running).unwrap();

    let all: Vec<_> = store
        .list(&SessionFilter::all())
        .unwrap()
        .into_iter()
        .map(|session| session.id)
        .collect();
    assert_eq!(all, [other.id.clone(), second.id.clone(), first.id.clone()]);
    let main = store
        .list(&SessionFilter::for_workspace(id("wsp_main")))
        .unwrap();
    assert_eq!(main.len(), 2);
    let running = store
        .list(&SessionFilter::all().with_state(SessionStateKind::Running))
        .unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].id, second.id);
    assert_eq!(
        store
            .list(&SessionFilter::all().with_limit(1))
            .unwrap()
            .len(),
        1
    );
    for limit in [0, 501] {
        assert!(matches!(
            store.list(&SessionFilter::all().with_limit(limit)),
            Err(SessionError::InvalidArgument(_))
        ));
    }
    assert!(matches!(
        store.get(&id("ses_missing")),
        Err(SessionError::NotFound {
            kind: "session",
            ..
        })
    ));
}

#[test]
fn messages_are_sequenced_and_paged() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "chat");
    for index in 0..5 {
        let message = store
            .append_message(
                &session.id,
                NewMessage::new(MessageRole::Assistant, format!("reply {index}")),
            )
            .unwrap();
        assert_eq!(message.seq, index + 1);
    }
    let tool = store
        .append_message(
            &session.id,
            NewMessage::new(MessageRole::Tool, "ok").for_tool_call(id("call_1")),
        )
        .unwrap();
    assert_eq!(tool.tool_call_id, Some(id("call_1")));
    let page = store.messages(&session.id, 0, 4).unwrap();
    assert_eq!(page.messages.len(), 4);
    assert!(page.has_more);
    assert_eq!(page.next_after, 4);
    let rest = store.messages(&session.id, page.next_after, 4).unwrap();
    assert_eq!(
        rest.messages.iter().map(|m| m.seq).collect::<Vec<_>>(),
        [5, 6]
    );
    assert!(!rest.has_more);
    assert!(store.messages(&session.id, 0, 0).is_err());
    let huge = "x".repeat(aidex_sessions::MAX_MESSAGE_BYTES + 1);
    assert!(matches!(
        store.append_message(&session.id, NewMessage::new(MessageRole::User, huge)),
        Err(SessionError::InvalidArgument(_))
    ));
}

#[test]
fn model_budget_and_cursor_are_recorded() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "budget");
    let model = ModelReference {
        provider_id: id("local"),
        model_id: id("gemma-4"),
    };
    let updated = store.set_model(&session.id, Some(model.clone())).unwrap();
    assert_eq!(updated.model, Some(model));
    let usage = BudgetUsage {
        input_tokens: 10,
        output_tokens: 5,
        tool_calls: 1,
        wall_ms: 300,
    };
    store.add_budget_usage(&session.id, usage).unwrap();
    let total = store.add_budget_usage(&session.id, usage).unwrap().budget;
    assert_eq!(total.input_tokens, 20);
    assert_eq!(total.wall_ms, 600);
    let saturated = store
        .add_budget_usage(
            &session.id,
            BudgetUsage {
                input_tokens: u64::MAX,
                ..BudgetUsage::default()
            },
        )
        .unwrap();
    assert_eq!(saturated.budget.input_tokens, i64::MAX as u64);
    let advanced = store
        .set_event_cursor(&session.id, Cursor::new(40))
        .unwrap();
    assert_eq!(advanced.last_event_cursor, Cursor::new(40));
    let kept = store.set_event_cursor(&session.id, Cursor::new(3)).unwrap();
    assert_eq!(kept.last_event_cursor, Cursor::new(40), "never moves back");
}

#[test]
fn subagent_sessions_must_share_the_parent_workspace() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let parent = new_session(&store, "parent");
    let child = store
        .create(
            NewSession::new(id("wsp_main"), id("agent_reviewer"), "review")
                .with_parent(parent.id.clone()),
        )
        .unwrap();
    assert_eq!(child.parent_session_id, Some(parent.id.clone()));
    let error = store
        .create(NewSession::new(id("wsp_other"), id("agent_x"), "x").with_parent(parent.id))
        .unwrap_err();
    assert!(matches!(error, SessionError::InvalidArgument(_)));
    assert!(matches!(
        store.create(NewSession::new(id("wsp_main"), id("agent_x"), "  ")),
        Err(SessionError::InvalidArgument(_))
    ));
}
