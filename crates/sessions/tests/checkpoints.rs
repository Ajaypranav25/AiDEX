//! Checkpoints: captured marks, hash verification, fallback past a corrupt latest
//! checkpoint, the all-corrupt error, and branching from a checkpoint.

mod common;

use std::fs;

use aidex_core::{ModelReference, Timestamp};
use aidex_sessions::{
    ApprovalRef, ApprovalScope, ApprovalStatus, BLOB_DIR, CheckpointStatus, LogicalState,
    MessageRole, NewMessage, NewToolCall, SessionError, SessionState, SessionStore, ToolCallState,
    ToolCallUpdate,
};
use common::{id, laptop, new_session, open, session_events, temp_drive};
use serde_json::json;

/// Flip one byte of a checkpoint blob on disk.
fn corrupt_blob(store: &SessionStore, sha256: &str) {
    let path = store
        .database_path()
        .parent()
        .unwrap()
        .join(BLOB_DIR)
        .join("artifacts")
        .join(sha256);
    let mut bytes = fs::read(&path).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x01;
    fs::write(&path, bytes).unwrap();
}

#[test]
fn checkpoints_capture_logical_state_and_high_water_marks() {
    let root = temp_drive();
    let (store, sink) = open(root.path(), laptop());
    let session = new_session(&store, "marks");
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    let model = ModelReference {
        provider_id: id("local"),
        model_id: id("gemma-4"),
    };
    store.set_model(&session.id, Some(model.clone())).unwrap();
    for text in ["plan it", "planned"] {
        store
            .append_message(&session.id, NewMessage::new(MessageRole::User, text))
            .unwrap();
    }
    store
        .record_tool_call(
            &session.id,
            NewToolCall::new(id("call_1"), id("fs.read"), json!({"path": "src/lib.rs"})),
        )
        .unwrap();
    store
        .record_approval_ref(
            &session.id,
            ApprovalRef {
                approval_id: id("apr_1"),
                intent_fingerprint_hex: "ab".repeat(32),
                decision: ApprovalStatus::Pending,
                scope: ApprovalScope::Once,
                expires_at: None,
            },
        )
        .unwrap();
    let meta = store
        .checkpoint(
            &session.id,
            Some("after planning"),
            LogicalState::new(json!({"phase": "implement"}))
                .with_task_graph(json!({"tasks": ["a", "b"]})),
        )
        .unwrap();
    assert!(meta.id.as_str().starts_with("chk_"));
    assert_eq!((meta.seq, meta.message_seq), (1, 2));
    assert_eq!(meta.sha256.len(), 64);
    assert_eq!(meta.status, CheckpointStatus::Valid);
    assert_eq!(
        session_events(&sink, &session.id).last(),
        Some(&"session.checkpointed")
    );

    let restored = store.latest_valid_checkpoint(&session.id).unwrap().unwrap();
    let checkpoint = restored.checkpoint;
    assert_eq!(checkpoint.meta, meta);
    assert_eq!(checkpoint.host_fingerprint, laptop());
    assert_eq!(checkpoint.state.harness, json!({"phase": "implement"}));
    assert_eq!(
        checkpoint.state.task_graph,
        Some(json!({"tasks": ["a", "b"]}))
    );
    let marks = checkpoint.marks;
    assert_eq!(marks.message_seq, 2);
    assert_eq!(marks.model, Some(model));
    assert_eq!(marks.open_approvals, [id("apr_1")]);
    assert_eq!(marks.tool_calls.len(), 1);
    assert_eq!(marks.tool_calls[0].state, ToolCallState::Requested);
    assert_eq!(
        marks.event_cursor.get(),
        2,
        "created + started before the checkpoint"
    );
    assert_eq!(store.checkpoints(&session.id).unwrap(), [meta]);
}

#[test]
fn a_corrupt_latest_checkpoint_falls_back_to_the_previous_valid_one() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "corrupt");
    let first = store
        .checkpoint(&session.id, Some("one"), LogicalState::new(json!({"n": 1})))
        .unwrap();
    let second = store
        .checkpoint(&session.id, Some("two"), LogicalState::new(json!({"n": 2})))
        .unwrap();
    corrupt_blob(&store, &second.sha256);

    let restored = store.latest_valid_checkpoint(&session.id).unwrap().unwrap();
    assert_eq!(restored.checkpoint.meta.id, first.id);
    assert_eq!(restored.checkpoint.state.harness, json!({"n": 1}));
    assert!(restored.recovered_from_corruption);
    assert_eq!(
        restored.corrupt_checkpoints,
        std::slice::from_ref(&second.id)
    );

    // The corruption is recorded durably, not rediscovered silently each time.
    drop(store);
    let (store, _) = open(root.path(), laptop());
    let metas = store.checkpoints(&session.id).unwrap();
    assert_eq!(metas[0].status, CheckpointStatus::Valid);
    assert_eq!(metas[1].status, CheckpointStatus::Corrupt);
    assert!(
        metas[1]
            .corrupt_reason
            .as_deref()
            .unwrap()
            .contains("mismatch")
    );
    let again = store.latest_valid_checkpoint(&session.id).unwrap().unwrap();
    assert_eq!(again.checkpoint.meta.id, first.id);
    assert!(again.recovered_from_corruption);
    let direct = store.load_checkpoint(&session.id, &second.id).unwrap_err();
    assert!(matches!(direct, SessionError::CheckpointCorrupt { .. }));
}

#[test]
fn every_checkpoint_corrupt_is_an_explicit_error() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "all corrupt");
    assert!(
        store
            .latest_valid_checkpoint(&session.id)
            .unwrap()
            .is_none()
    );
    let one = store
        .checkpoint(&session.id, None, LogicalState::new(json!({"n": 1})))
        .unwrap();
    let two = store
        .checkpoint(&session.id, None, LogicalState::new(json!({"n": 2})))
        .unwrap();
    corrupt_blob(&store, &one.sha256);
    // A deleted blob is as unrecoverable as a damaged one.
    fs::remove_file(
        store
            .database_path()
            .parent()
            .unwrap()
            .join(BLOB_DIR)
            .join("artifacts")
            .join(&two.sha256),
    )
    .unwrap();
    let error = store.latest_valid_checkpoint(&session.id).unwrap_err();
    assert!(
        matches!(error, SessionError::NoValidCheckpoint { corrupt: 2, .. }),
        "{error}"
    );
    assert_eq!(error.reason(), "no_valid_checkpoint");
    assert!(
        store
            .checkpoints(&session.id)
            .unwrap()
            .iter()
            .all(|meta| meta.status == CheckpointStatus::Corrupt)
    );
}

#[test]
fn branching_seeds_a_child_from_a_checkpoint_without_inheriting_approvals() {
    let root = temp_drive();
    let (store, sink) = open(root.path(), laptop());
    let parent = new_session(&store, "try gemma");
    store.transition(&parent.id, SessionState::Running).unwrap();
    for text in ["one", "two"] {
        store
            .append_message(&parent.id, NewMessage::new(MessageRole::User, text))
            .unwrap();
    }
    store
        .record_approval_ref(
            &parent.id,
            ApprovalRef {
                approval_id: id("apr_session"),
                intent_fingerprint_hex: "cd".repeat(32),
                decision: ApprovalStatus::Approved,
                scope: ApprovalScope::Session,
                expires_at: Some(Timestamp::from_unix_millis(4_102_444_800_000).unwrap()),
            },
        )
        .unwrap();
    let point = store
        .checkpoint(
            &parent.id,
            Some("checkpoint 4"),
            LogicalState::new(json!({"plan": "A"})),
        )
        .unwrap();
    store
        .append_message(
            &parent.id,
            NewMessage::new(MessageRole::User, "after the checkpoint"),
        )
        .unwrap();
    store
        .transition(&parent.id, SessionState::Completed)
        .unwrap();

    let child = store.branch(&parent.id, &point.id).unwrap();
    assert_ne!(child.id, parent.id);
    assert_eq!(child.state, SessionState::Created);
    assert_eq!(child.parent_session_id, Some(parent.id.clone()));
    assert_eq!(child.branched_from, Some(point.id.clone()));
    assert_eq!(child.workspace_id, parent.workspace_id);
    assert_eq!(child.title, "branch of try gemma");
    let messages = store.messages(&child.id, 0, 10).unwrap().messages;
    assert_eq!(
        messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
    assert!(store.approvals(&child.id).unwrap().is_empty());
    let seed = store
        .latest_valid_checkpoint(&child.id)
        .unwrap()
        .unwrap()
        .checkpoint;
    assert_eq!(seed.seeded_from, Some(point.id.clone()));
    assert_eq!(seed.state.harness, json!({"plan": "A"}));
    assert!(seed.marks.open_approvals.is_empty());
    assert_eq!(
        session_events(&sink, &child.id),
        ["session.created", "session.checkpointed"]
    );
    // The child continues independently.
    store.transition(&child.id, SessionState::Running).unwrap();
    let next = store
        .append_message(
            &child.id,
            NewMessage::new(MessageRole::User, "try codex instead"),
        )
        .unwrap();
    assert_eq!(next.seq, 3);

    let missing = store.branch(&parent.id, &id("chk_missing")).unwrap_err();
    assert!(matches!(
        missing,
        SessionError::NotFound {
            kind: "checkpoint",
            ..
        }
    ));
    let foreign = store.branch(&child.id, &point.id).unwrap_err();
    assert!(matches!(
        foreign,
        SessionError::NotFound {
            kind: "checkpoint",
            ..
        }
    ));
}

#[test]
fn tool_calls_record_results_artifacts_and_reject_late_changes() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "tools");
    store
        .record_tool_call(
            &session.id,
            NewToolCall::new(
                id("call_1"),
                id("shell.exec"),
                json!({"command": "cargo test"}),
            ),
        )
        .unwrap();
    let duplicate = store
        .record_tool_call(
            &session.id,
            NewToolCall::new(id("call_1"), id("shell.exec"), json!({})),
        )
        .unwrap_err();
    assert!(matches!(duplicate, SessionError::Conflict(_)));
    let running = store
        .update_tool_call(
            &session.id,
            &id("call_1"),
            ToolCallUpdate::state(ToolCallState::Running).with_process(id("proc_1")),
        )
        .unwrap();
    assert!(running.started_at.is_some());
    let artifact = store
        .put_artifact(
            &session.id,
            aidex_sessions::NewArtifact::new(
                aidex_sessions::ArtifactKind::TestReport,
                id("shell.exec"),
                b"test result: ok. 12 passed".to_vec(),
            ),
        )
        .unwrap();
    let done = store
        .update_tool_call(
            &session.id,
            &id("call_1"),
            ToolCallUpdate::state(ToolCallState::Completed)
                .with_summary("12 passed")
                .with_artifact(artifact.id.clone()),
        )
        .unwrap();
    assert_eq!(done.state, ToolCallState::Completed);
    assert!(done.finished_at.is_some());
    assert_eq!(done.output_artifact.as_ref().unwrap().id, artifact.id);
    assert_eq!(done.process_id, Some(id("proc_1")));
    let late = store
        .update_tool_call(
            &session.id,
            &id("call_1"),
            ToolCallUpdate::state(ToolCallState::Failed),
        )
        .unwrap_err();
    assert!(matches!(late, SessionError::Conflict(_)));
    assert_eq!(store.tool_calls(&session.id).unwrap(), [done]);
    assert_eq!(
        store.read_artifact(&artifact.id).unwrap(),
        b"test result: ok. 12 passed"
    );
}
