//! `workspace.*` over authenticated IPC: selection and identity survive a daemon
//! restart, and ephemeral review/apply/discard work end to end.

mod common;

use aidex_workspace::{ApplyReport, EphemeralCreated, Workspace};
use aidexd::{
    Client, ErrorCode,
    methods::workspace::{
        ADD, ApplyParams, EPHEMERAL_APPLY, EPHEMERAL_CREATE, EPHEMERAL_DISCARD, EPHEMERAL_REVIEW,
        EphemeralReview, LIST, SELECT, SELECTED, SelectedWorkspace, WorkspaceList,
    },
};
use common::{options, start};
use serde_json::json;
use std::fs;

#[test]
fn selection_and_identity_survive_a_daemon_restart() {
    let root = common::temp_drive();
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("README.md"), "hello").unwrap();

    let daemon = start(root.path(), options(), |_| {});
    let mut client = Client::connect(root.path(), "test-workspace").unwrap();
    let added: Workspace = client
        .call_as(
            ADD,
            json!({"path": project.path().to_str().unwrap(), "name": "project"}),
        )
        .unwrap();
    let selected: Workspace = client
        .call_as(SELECT, json!({"id_or_name": "project"}))
        .unwrap();
    assert_eq!(selected.id, added.id);
    let duplicate = client
        .call(ADD, json!({"path": project.path().to_str().unwrap()}))
        .unwrap_err();
    assert_eq!(duplicate.rpc_kind(), Some(ErrorCode::Conflict));
    drop(client);
    daemon.stop();
    daemon.wait().unwrap();

    let restarted = start(root.path(), options(), |_| {});
    let mut client = Client::connect(root.path(), "test-workspace").unwrap();
    let list: WorkspaceList = client.call_as(LIST, json!({})).unwrap();
    assert_eq!(list.workspaces.len(), 1);
    assert_eq!(list.workspaces[0].id, added.id);
    assert_eq!(list.selected, Some(added.id.clone()));
    let current: SelectedWorkspace = client.call_as(SELECTED, json!({})).unwrap();
    assert_eq!(current.workspace.unwrap().id, added.id);
    restarted.stop();
    restarted.wait().unwrap();
}

#[test]
fn ephemeral_changes_are_reviewed_then_applied_selectively_over_ipc() {
    let root = common::temp_drive();
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("a.txt"), "a\n").unwrap();
    fs::write(project.path().join("b.txt"), "b\n").unwrap();
    let daemon = start(root.path(), options(), |_| {});
    let mut client = Client::connect(root.path(), "test-workspace").unwrap();
    let source: Workspace = client
        .call_as(ADD, json!({"path": project.path().to_str().unwrap()}))
        .unwrap();
    let created: EphemeralCreated = client
        .call_as(EPHEMERAL_CREATE, json!({"source": source.id.as_str()}))
        .unwrap();
    let tree = std::path::PathBuf::from(&created.workspace.path);
    fs::write(tree.join("a.txt"), "a changed\n").unwrap();
    fs::write(tree.join("b.txt"), "b changed\n").unwrap();

    let review: EphemeralReview = client
        .call_as(
            EPHEMERAL_REVIEW,
            json!({"id": created.workspace.id.as_str()}),
        )
        .unwrap();
    assert_eq!(review.total_changes, 2);
    assert!(!review.truncated);
    let params = ApplyParams {
        id: created.workspace.id.to_string(),
        paths: Some(vec![review.changes[0].path.clone()]),
        review_digest: Some(review.digest.clone()),
    };
    let report: ApplyReport = client
        .call_as(EPHEMERAL_APPLY, serde_json::to_value(&params).unwrap())
        .unwrap();
    assert_eq!(report.applied.len(), 1);
    assert_eq!(
        fs::read_to_string(project.path().join("a.txt")).unwrap(),
        "a changed\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("b.txt")).unwrap(),
        "b\n"
    );

    // The stale digest is refused now that the review changed.
    let stale = client
        .call(EPHEMERAL_APPLY, serde_json::to_value(&params).unwrap())
        .unwrap_err();
    assert_eq!(stale.rpc_kind(), Some(ErrorCode::Conflict));

    client
        .call(
            EPHEMERAL_DISCARD,
            json!({"id": created.workspace.id.as_str()}),
        )
        .unwrap();
    assert!(!tree.exists());
    assert_eq!(
        fs::read_to_string(project.path().join("b.txt")).unwrap(),
        "b\n"
    );
    daemon.stop();
    daemon.wait().unwrap();
}
