//! Local inference over IPC: streamed model import, verification, the local
//! provider in `model.list`, runtime inventory and planning errors with advice,
//! guarded removal, and the reserved `local` provider id.

mod common;

use aidex_inference::{
    LocalModelId, RegistryEntry, RemovedModel, RuntimeInventory, VerificationMethod, Verified,
    gguf::GgufBuilder,
};
use aidexd::{
    Client, ClientError, ErrorCode,
    methods::{
        inference::{
            IMPORT_DONE, IMPORT_FAILED, IMPORT_PROGRESS, LoadedList, MODEL_IMPORT, MODEL_LOAD,
            MODEL_LOADED, MODEL_REMOVE, MODEL_RUN, MODEL_UNLOAD, MODEL_VERIFY, RUN_FAILED,
            RUNTIME_INSTALL, RUNTIME_LIST, RUNTIME_SELECT, StreamFailure, Unloaded,
        },
        models::{MODEL_LIST, ModelList, PROVIDER_ADD},
    },
};
use common::{options, start};
use serde_json::{Value, json};
use std::time::Duration;

/// Collect stream notifications until the stream ends.
fn drain(client: &mut Client, method: &str, params: Value) -> Vec<(String, Value)> {
    let mut subscription = client.subscribe(method, params).unwrap();
    assert_eq!(subscription.initial()["started"], true);
    let mut seen = Vec::new();
    for _ in 0..1000 {
        match subscription.next_timeout(Duration::from_secs(30)).unwrap() {
            Some(notification) => seen.push((notification.method, notification.params)),
            None if subscription.is_ended() => break,
            None => panic!("stream stalled after {seen:?}"),
        }
    }
    seen
}

fn rpc_kind(error: &ClientError) -> (Option<ErrorCode>, String) {
    let ClientError::Rpc { data, .. } = error else {
        panic!("{error:?}");
    };
    let kind = data
        .as_ref()
        .and_then(|data| data["kind"].as_str())
        .unwrap_or_default()
        .to_owned();
    (error.rpc_kind(), kind)
}

#[test]
fn models_are_imported_verified_listed_planned_and_removed_over_ipc() {
    let root = common::temp_drive();
    let daemon = start(root.path(), options(), |_| {});
    let sources = tempfile::tempdir().unwrap();
    let file = sources.path().join("Tiny-Coder.gguf");
    GgufBuilder::chat_model("qwen2", "Tiny Coder")
        .tensor("big", &[1024, 1024], 1)
        .write_to(&file)
        .unwrap();
    let mut client = Client::connect(root.path(), "test-inference").unwrap();

    let notifications = drain(
        &mut client,
        MODEL_IMPORT,
        json!({"path": file, "license": "Apache-2.0", "source": "https://example.com/tiny"}),
    );
    let methods: Vec<&str> = notifications
        .iter()
        .map(|(method, _)| method.as_str())
        .collect();
    assert!(methods.contains(&IMPORT_PROGRESS), "{methods:?}");
    let (last, data) = notifications.last().unwrap();
    assert_eq!(last, IMPORT_DONE, "{notifications:?}");
    let entry: RegistryEntry = serde_json::from_value(data["entry"].clone()).unwrap();
    assert_eq!(entry.id.as_str(), "tiny-coder");
    assert_eq!(entry.license.name, "Apache-2.0");

    // Importing the same id again fails on the stream without touching the model.
    let failed = drain(&mut client, MODEL_IMPORT, json!({"path": file}));
    let (last, data) = failed.last().unwrap();
    assert_eq!(last, IMPORT_FAILED);
    let failure: StreamFailure = serde_json::from_value(data.clone()).unwrap();
    assert_eq!(failure.error.kind(), Some(ErrorCode::Conflict));
    assert_eq!(failure.error.data.unwrap()["kind"], "already_exists");

    let verified: Verified = client
        .call_as(MODEL_VERIFY, json!({"id": "tiny-coder", "force": true}))
        .unwrap();
    assert_eq!(verified.method, VerificationMethod::FullHash);
    assert_eq!(verified.sha256, entry.sha256);

    let listing: ModelList = client.call_as(MODEL_LIST, Value::Null).unwrap();
    let local = listing
        .models
        .iter()
        .find(|model| model.model.to_string() == "local/tiny-coder")
        .expect("the local provider lists imported models");
    assert_eq!(local.locality.as_str(), "local");

    let inventory: RuntimeInventory = client.call_as(RUNTIME_LIST, Value::Null).unwrap();
    assert_eq!(
        inventory.platform.as_deref(),
        aidex_drive::platform_runtime_name()
    );
    assert!(inventory.installed.runtimes.is_empty());
    if inventory.platform.as_deref() == Some("windows-x64") {
        assert!(
            inventory
                .catalog
                .iter()
                .any(|entry| entry.archive == "llama-b11205-bin-win-cpu-x64.zip" && entry.validated)
        );
    }

    for method in [RUNTIME_SELECT, MODEL_LOAD] {
        let error = client
            .call(method, json!({"id": "tiny-coder"}))
            .unwrap_err();
        let (code, kind) = rpc_kind(&error);
        assert_eq!(code, Some(ErrorCode::ModelUnavailable), "{method}: {error}");
        assert_eq!(kind, "runtime_not_installed");
        let ClientError::Rpc { data, .. } = &error else {
            unreachable!()
        };
        assert!(
            data.as_ref().unwrap()["advice"]
                .as_str()
                .unwrap()
                .contains("aidex runtime install")
        );
    }
    let unloaded: Unloaded = client
        .call_as(MODEL_UNLOAD, json!({"id": "tiny-coder"}))
        .unwrap();
    assert!(!unloaded.unloaded);
    let loaded: LoadedList = client.call_as(MODEL_LOADED, Value::Null).unwrap();
    assert!(loaded.models.is_empty());

    let refused = client
        .call(MODEL_REMOVE, json!({"id": "tiny-coder"}))
        .unwrap_err();
    assert_eq!(refused.rpc_kind(), Some(ErrorCode::InvalidParams));
    let removed: RemovedModel = client
        .call_as(MODEL_REMOVE, json!({"id": "tiny-coder", "confirm": true}))
        .unwrap();
    assert_eq!(removed.id, LocalModelId::new("tiny-coder").unwrap());
    assert!(!root.path().join("models/tiny-coder/model.gguf").exists());
    let missing = client
        .call(MODEL_VERIFY, json!({"id": "tiny-coder"}))
        .unwrap_err();
    assert_eq!(
        rpc_kind(&missing),
        (Some(ErrorCode::NotFound), "model_not_found".into())
    );

    drop(client);
    daemon.stop();
    daemon.wait().unwrap();
}

#[test]
fn invalid_requests_fail_explicitly() {
    let root = common::temp_drive();
    let daemon = start(root.path(), options(), |_| {});
    let mut client = Client::connect(root.path(), "test-inference").unwrap();

    let failed = drain(
        &mut client,
        MODEL_IMPORT,
        json!({"path": root.path().join("missing.gguf")}),
    );
    assert_eq!(failed.last().unwrap().0, IMPORT_FAILED);

    let failed = drain(
        &mut client,
        MODEL_RUN,
        json!({"id": "nope", "prompt": "hi"}),
    );
    let (last, data) = failed.last().unwrap();
    assert_eq!(last, RUN_FAILED);
    let failure: StreamFailure = serde_json::from_value(data.clone()).unwrap();
    assert_eq!(failure.error.kind(), Some(ErrorCode::NotFound));
    assert_eq!(
        client
            .call(MODEL_RUN, json!({"id": "nope", "prompt": " "}))
            .unwrap_err()
            .rpc_kind(),
        Some(ErrorCode::InvalidParams)
    );

    let archive = root.path().join("random.zip");
    std::fs::write(&archive, b"not a pinned archive").unwrap();
    let error = client
        .call(RUNTIME_INSTALL, json!({"archive": archive}))
        .unwrap_err();
    assert_eq!(
        rpc_kind(&error),
        (
            Some(ErrorCode::InvalidArgument),
            "runtime_not_in_catalog".into()
        )
    );
    assert_eq!(
        client
            .call(MODEL_LOAD, json!({"id": "Not Valid"}))
            .unwrap_err()
            .rpc_kind(),
        Some(ErrorCode::InvalidArgument)
    );
    let reserved = client
        .call(
            PROVIDER_ADD,
            json!({"id": "local", "base_url": "http://127.0.0.1:9/v1"}),
        )
        .unwrap_err();
    assert_eq!(reserved.rpc_kind(), Some(ErrorCode::InvalidParams));

    drop(client);
    daemon.stop();
    daemon.wait().unwrap();
}
