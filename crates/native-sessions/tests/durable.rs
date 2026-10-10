use aidex_native_sessions::{RunStatus, SessionConfig, SessionStore};
use aidex_vault::{KdfParams, SecretString, VaultService, VaultServiceConfig};
use std::sync::Arc;

fn fixture() -> (tempfile::TempDir, Arc<VaultService>, SessionStore) {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(
        VaultService::open(
            dir.path().join("vault"),
            VaultServiceConfig {
                kdf: KdfParams::new(8, 1, 1).unwrap(),
                idle_timeout: None,
            },
        )
        .unwrap(),
    );
    vault
        .init(&SecretString::from("fixture-passphrase".to_owned()), false)
        .unwrap();
    let store = SessionStore::open(dir.path().join("sessions.sqlite3"), vault.clone()).unwrap();
    (dir, vault, store)
}
fn config() -> SessionConfig {
    serde_json::from_value(serde_json::json!({"title":"private fixture title","mode":"chat","provider_id":"fixture","model_id":"small"})).unwrap()
}

#[test]
fn history_is_encrypted_locked_and_reloaded() {
    let (dir, vault, store) = fixture();
    let session = store.create(config()).unwrap();
    let run = store
        .begin(&session.id, "private fixture prompt", "command-1", None)
        .unwrap();
    store
        .update(&session.id, |s| {
            s.runs[0].status = RunStatus::Running;
            Ok(())
        })
        .unwrap();
    let bytes = std::fs::read(dir.path().join("sessions.sqlite3")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("private fixture"));
    vault.lock();
    assert!(store.get(&session.id).is_err());
    assert!(store.list().unwrap()[0].title.is_none());
    drop(store);
    let reopened = SessionStore::open(dir.path().join("sessions.sqlite3"), vault.clone()).unwrap();
    vault
        .unlock(&SecretString::from("fixture-passphrase".to_owned()))
        .unwrap();
    let detail = reopened.get(&session.id).unwrap();
    assert_eq!(detail.messages[0].content, "private fixture prompt");
    assert_eq!(detail.runs[0].id, run.id);
    assert_eq!(detail.runs[0].status, RunStatus::Interrupted);
}
#[test]
fn duplicate_message_command_returns_original_run() {
    let (_drive, _vault, store) = fixture();
    let s = store.create(config()).unwrap();
    let a = store.begin(&s.id, "hello", "once", None).unwrap();
    let b = store.begin(&s.id, "hello", "once", None).unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(store.get(&s.id).unwrap().messages.len(), 1);
    assert!(store.begin(&s.id, "different", "once", None).is_err());
}
#[test]
fn memory_is_private_scoped_durable_and_locked() {
    let (dir, vault, store) = fixture();
    let record = store
        .memory_set("workspace-a", "notes", "private memory fixture")
        .unwrap();
    assert_eq!(
        store.memory_list("workspace-a").unwrap()[0].content,
        "private memory fixture"
    );
    assert!(store.memory_list("workspace-b").unwrap().is_empty());
    assert!(
        !String::from_utf8_lossy(&std::fs::read(dir.path().join("sessions.sqlite3")).unwrap())
            .contains("private memory fixture")
    );
    vault.lock();
    assert!(store.memory_list("workspace-a").is_err());
    vault
        .unlock(&SecretString::from("fixture-passphrase".to_owned()))
        .unwrap();
    drop(store);
    let reopened = SessionStore::open(dir.path().join("sessions.sqlite3"), vault).unwrap();
    assert_eq!(
        reopened.memory_list("workspace-a").unwrap()[0].id,
        record.id
    );
    assert!(reopened.memory_delete("workspace-a", &record.id).unwrap());
    assert!(reopened.memory_list("workspace-a").unwrap().is_empty());
}
#[test]
fn instructions_are_private_and_invalid_fallback_is_refused() {
    let (_drive, _vault, store) = fixture();
    let mut value = serde_json::to_value(config()).unwrap();
    value["instructions"] = serde_json::json!("user guidance");
    let parsed: SessionConfig = serde_json::from_value(value.clone()).unwrap();
    let s = store.create(parsed).unwrap();
    assert_eq!(
        serde_json::to_value(store.get(&s.id).unwrap()).unwrap()["instructions"],
        "user guidance"
    );
    value["fallback_model"] = serde_json::json!("invalid reference");
    let parsed: SessionConfig = serde_json::from_value(value).unwrap();
    assert!(store.create(parsed).is_err());
}
#[test]
fn restarting_a_waiting_run_marks_it_interrupted() {
    let (dir, vault, store) = fixture();
    let s = store.create(config()).unwrap();
    let r = store.begin(&s.id, "pending action", "once", None).unwrap();
    store
        .update(&s.id, |s| {
            s.runs[0].status = RunStatus::WaitingApproval;
            Ok(())
        })
        .unwrap();
    drop(store);
    let reopened = SessionStore::open(dir.path().join("sessions.sqlite3"), vault).unwrap();
    assert_eq!(reopened.run(&r.id).unwrap().status, RunStatus::Interrupted);
}
