//! End-to-end `aidex session` checks against an in-process daemon: a session that was
//! running when the daemon stopped is listed as interrupted, `resume` prints the plan
//! and adopts it without pretending to continue, and branch/cancel/errors behave.

use aidex_core::EntityId;
use aidex_sessions::{LogicalState, MessageRole, NewMessage, SessionState};
use aidexd::Daemon;
use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};

fn aidex(args: &[&str], drive: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aidex"))
        .args(args)
        .args(["--drive", drive.to_str().expect("utf-8 temp path")])
        .env_remove("AIDEX_DRIVE_ROOT")
        .output()
        .expect("run aidex")
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

fn ok(output: &Output) -> String {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn ok_json(output: &Output) -> Value {
    serde_json::from_str(&ok(output)).expect("stdout is JSON")
}

#[test]
fn interrupted_sessions_are_listed_resumed_branched_and_cancelled() {
    let drive = temp_drive();
    let daemon = Daemon::new(drive.path()).unwrap();
    let store = daemon.services().sessions().clone();
    let daemon = daemon.start().unwrap();
    let workspace = ok_json(&aidex(
        &["--json", "workspace", "create", "project"],
        drive.path(),
    ));
    let workspace = workspace["id"].as_str().unwrap().to_owned();
    let created = ok_json(&aidex(
        &[
            "--json",
            "session",
            "create",
            "--workspace",
            &workspace,
            "add",
            "a",
            "flag",
        ],
        drive.path(),
    ));
    assert_eq!(created["title"], "add a flag");
    assert_eq!(created["state"]["kind"], "created");
    let id = EntityId::new(created["id"].as_str().unwrap()).unwrap();
    // Simulate the harness: run, talk, checkpoint; then the daemon stops mid-task.
    store.transition(&id, SessionState::Running).unwrap();
    store
        .append_message(&id, NewMessage::new(MessageRole::User, "add --verbose"))
        .unwrap();
    let checkpoint = store
        .checkpoint(
            &id,
            Some("planned"),
            LogicalState::new(serde_json::json!({})),
        )
        .unwrap();
    store
        .append_message(
            &id,
            NewMessage::new(MessageRole::Assistant, "editing cli.rs"),
        )
        .unwrap();
    drop(store);
    daemon.stop();
    daemon.wait().unwrap();

    let restarted = Daemon::new(drive.path()).unwrap().start().unwrap();
    let list = ok(&aidex(
        &["session", "list", "--state", "interrupted"],
        drive.path(),
    ));
    assert!(
        list.contains(&format!("{id}  interrupted  {workspace}")),
        "{list}"
    );
    let shown = ok(&aidex(&["session", "show", id.as_str()], drive.path()));
    assert!(
        shown.contains("State: interrupted (recoverable): daemon restarted"),
        "{shown}"
    );
    let checkpoints = ok(&aidex(
        &["session", "checkpoints", id.as_str()],
        drive.path(),
    ));
    assert!(
        checkpoints.contains(checkpoint.id.as_str()),
        "{checkpoints}"
    );
    let messages = ok_json(&aidex(
        &["--json", "session", "messages", id.as_str(), "--after", "1"],
        drive.path(),
    ));
    assert_eq!(messages["messages"][0]["content"], "editing cli.rs");

    let resumed = ok(&aidex(&["session", "resume", id.as_str()], drive.path()));
    assert!(resumed.contains("Resume from: #1"), "{resumed}");
    assert!(
        resumed.contains("Messages since the checkpoint: 1"),
        "{resumed}"
    );
    assert!(resumed.contains("Adopted on this host"), "{resumed}");
    assert!(
        resumed.contains("Continuation: continuation is not available yet"),
        "{resumed}"
    );
    let again = ok_json(&aidex(
        &["--json", "session", "resume", id.as_str()],
        drive.path(),
    ));
    assert_eq!(again["adopted"], true);
    assert_eq!(again["continuation_available"], false);
    assert_eq!(again["session"]["state"]["kind"], "paused");

    let branch = ok_json(&aidex(
        &[
            "--json",
            "session",
            "branch",
            id.as_str(),
            "--checkpoint",
            checkpoint.id.as_str(),
        ],
        drive.path(),
    ));
    assert_eq!(branch["parent_session_id"], id.as_str());
    let cancelled = ok_json(&aidex(
        &["--json", "session", "cancel", id.as_str()],
        drive.path(),
    ));
    assert_eq!(cancelled["state"]["kind"], "cancelled");
    let not_resumable = aidex(&["session", "resume", id.as_str()], drive.path());
    assert_eq!(not_resumable.status.code(), Some(1));
    let missing = aidex(&["--json", "session", "show", "ses_missing"], drive.path());
    assert_eq!(missing.status.code(), Some(8));
    let bad_state = aidex(&["session", "list", "--state", "sleeping"], drive.path());
    assert_eq!(bad_state.status.code(), Some(2));
    restarted.stop();
    restarted.wait().unwrap();
}
