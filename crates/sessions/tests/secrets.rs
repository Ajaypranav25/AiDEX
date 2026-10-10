//! Session records never keep secret-like values: every persisted text field is
//! redacted (defence in depth; credentials belong in the vault, passed by handle).

mod common;

use aidex_sessions::{
    ArtifactKind, LogicalState, MessageRole, NewArtifact, NewMessage, NewSession, NewToolCall,
    REDACTED, SessionState, ToolCallState, ToolCallUpdate, contains_secret_like,
};
use common::{all_bytes, contains, id, laptop, new_session, open, temp_drive};
use serde_json::json;

const GITHUB_TOKEN: &str = "ghp_R4nd0mT0kenValue1234567890abcdefAB";
const OPENAI_KEY: &str = "sk-proj-Abcdefghijklmnopqrstuvwxyz012345";
const PASSWORD: &str = "correct-horse-battery";

#[test]
fn secret_like_strings_are_detected() {
    assert!(contains_secret_like(GITHUB_TOKEN));
    assert!(contains_secret_like(&format!(
        "OPENAI_API_KEY={OPENAI_KEY}"
    )));
    assert!(contains_secret_like(&format!("password: {PASSWORD}")));
    assert!(!contains_secret_like("cargo test --workspace"));
}

#[test]
fn secret_values_are_redacted_in_every_persisted_record() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = store
        .create(NewSession::new(
            id("wsp_main"),
            id("agent_native"),
            format!("deploy with {GITHUB_TOKEN}"),
        ))
        .unwrap();
    assert!(!session.title.contains(GITHUB_TOKEN));
    let message = store
        .append_message(
            &session.id,
            NewMessage::new(MessageRole::User, format!("use key {OPENAI_KEY} please")),
        )
        .unwrap();
    assert!(message.redacted);
    assert_eq!(message.content, format!("use key {REDACTED} please"));

    let call = store
        .record_tool_call(
            &session.id,
            NewToolCall::new(
                id("call_push"),
                id("shell.exec"),
                json!({
                    "command": format!("git push https://x:{GITHUB_TOKEN}@github.com/o/r"),
                    "env": {"password": PASSWORD},
                    "cwd": "repo",
                }),
            ),
        )
        .unwrap();
    assert!(call.redacted);
    let input = call.input.to_string();
    assert!(!input.contains(GITHUB_TOKEN), "{input}");
    assert!(!input.contains(PASSWORD), "{input}");
    assert_eq!(call.input["cwd"], "repo");
    let finished = store
        .update_tool_call(
            &session.id,
            &id("call_push"),
            ToolCallUpdate::state(ToolCallState::Failed).with_summary(format!(
                "remote rejected Authorization: Bearer {OPENAI_KEY}"
            )),
        )
        .unwrap();
    assert!(!finished.result_summary.unwrap().contains(OPENAI_KEY));

    let artifact = store
        .put_artifact(
            &session.id,
            NewArtifact::new(
                ArtifactKind::CommandOutput,
                id("shell.exec"),
                format!("export GITHUB_TOKEN={GITHUB_TOKEN}\nok\n").into_bytes(),
            ),
        )
        .unwrap();
    assert!(artifact.redacted);
    let stored = String::from_utf8(store.read_artifact(&artifact.id).unwrap()).unwrap();
    assert!(!stored.contains(GITHUB_TOKEN));
    assert!(stored.ends_with("ok\n"));

    store
        .checkpoint(
            &session.id,
            Some(&format!("token {GITHUB_TOKEN}")),
            LogicalState::new(json!({"notes": format!("api_key={OPENAI_KEY}")})),
        )
        .unwrap();
    store
        .transition(&session.id, SessionState::Running)
        .unwrap();
    store
        .transition(
            &session.id,
            SessionState::Failed {
                code: "push.rejected".into(),
                message: format!("token {GITHUB_TOKEN} was revoked"),
            },
        )
        .unwrap();
    let restored = store.latest_valid_checkpoint(&session.id).unwrap().unwrap();
    assert!(
        !restored
            .checkpoint
            .state
            .harness
            .to_string()
            .contains(OPENAI_KEY)
    );
    let database = store.database_path().to_path_buf();
    drop(store);

    // Nothing on disk — SQLite pages, WAL, blobs — holds the values.
    let bytes = all_bytes(database.parent().unwrap());
    for secret in [GITHUB_TOKEN, OPENAI_KEY, PASSWORD] {
        assert!(!contains(&bytes, secret), "{secret} reached the drive");
    }
}

#[test]
fn binary_artifacts_are_stored_unchanged() {
    let root = temp_drive();
    let (store, _) = open(root.path(), laptop());
    let session = new_session(&store, "binary");
    let png = vec![0x89, b'P', b'N', b'G', 0xff, 0x00, 0xfe];
    let artifact = store
        .put_artifact(
            &session.id,
            NewArtifact::new(ArtifactKind::Screenshot, id("browser"), png.clone()),
        )
        .unwrap();
    assert!(!artifact.redacted);
    assert_eq!(store.read_artifact(&artifact.id).unwrap(), png);
    assert_eq!(store.artifacts(&session.id).unwrap(), [artifact]);
}
