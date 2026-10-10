//! `update.*` over authenticated IPC: signed apply, typed refusals, status, rollback,
//! and recovery of an interrupted update when the daemon starts.

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use aidex_bundle::{
    BundleFile, BundleManifest, FileRole, KeyPair, KeySource, LAUNCHER_MARKER, MANIFEST_FILE_NAME,
    MANIFEST_SCHEMA_VERSION, PRODUCT_NAME, StatePaths, Target, TrustStore, VerifyScope,
    archive::{ArchiveFormat, archive_file_name, bundle_root_name, write_archive},
    layout, read_manifest_bytes, sign_manifest, verify_bundle, write_signature,
};
use aidex_drive::DrivePaths;
use aidex_updater::{
    ApplyOptions, Checkpoint, Interrupted, SelfTestFn, SelfTestReport, UpdateSource, Updater,
    UpdaterConfig,
};
use aidexd::{
    Client, ErrorCode,
    methods::update::{APPLY, ApplyReport, CHECK, CheckReport, ROLLBACK, STATUS, StatusResult},
};
use common::{options, start};
use serde_json::{Value, json};

fn host() -> Target {
    Target::host().expect("a named host target")
}

fn config(key: &KeyPair) -> UpdaterConfig {
    let trust = TrustStore::empty().with_key(key.trusted_key(KeySource::Explicit));
    UpdaterConfig::new(trust).with_self_test(Arc::new(SelfTestFn(
        |_: &Path, _: &BundleManifest, _: &Path| Ok(SelfTestReport::default()),
    )))
}

/// A synthetic bundle archive for the host, signed by `key` unless `None`.
fn package(out: &Path, version: &str, key: Option<&KeyPair>) -> PathBuf {
    let target = host();
    let dir = out.join("src").join(bundle_root_name(version, target));
    fs::create_dir_all(dir.join("bin")).unwrap();
    let mut files = Vec::new();
    for (role, stem) in [
        (FileRole::Launcher, layout::LAUNCHER_BINARY),
        (FileRole::Cli, layout::CLI_BINARY),
        (FileRole::Daemon, layout::DAEMON_BINARY),
        (FileRole::Helper, layout::HELPER_BINARY),
    ] {
        let path = layout::bin_path(stem, target);
        fs::write(
            dir.join(&path),
            format!("{stem} {version}\n{LAUNCHER_MARKER}\n"),
        )
        .unwrap();
        files.push(BundleFile::describe(&dir, &path, role, true).unwrap());
    }
    let mut manifest = BundleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        product: PRODUCT_NAME.into(),
        version: version.into(),
        git_commit: None,
        source_dirty: None,
        target,
        rust_target: target.default_rust_triple().into(),
        rustc: None,
        created_at: aidex_core::Timestamp::from_unix_millis(1_760_000_000_000).unwrap(),
        min_drive_schema: aidex_drive::CURRENT_SCHEMA_VERSION,
        state: StatePaths::current(),
        files,
        licenses: Vec::new(),
    };
    manifest.normalize();
    fs::write(dir.join(MANIFEST_FILE_NAME), manifest.to_json().unwrap()).unwrap();
    if let Some(key) = key {
        let bytes = read_manifest_bytes(&dir).unwrap();
        write_signature(&dir, &sign_manifest(&bytes, key)).unwrap();
    }
    let path = out.join(archive_file_name(version, target));
    write_archive(
        &verify_bundle(&dir, VerifyScope::All).unwrap(),
        &path,
        ArchiveFormat::for_target(target),
    )
    .unwrap();
    path
}

fn source(path: &Path) -> Value {
    json!({ "source": path.to_str().unwrap() })
}

#[test]
fn apply_status_refusal_and_rollback_over_ipc() {
    let root = common::temp_drive();
    let out = tempfile::tempdir().unwrap();
    let key = KeyPair::generate("daemon-tests").unwrap();
    let mut opts = options();
    opts.updater = Some(config(&key));
    let _daemon = start(root.path(), opts, |_| {});
    let mut client = Client::connect(root.path(), "test-update").unwrap();

    let v1 = package(&out.path().join("v1"), "0.1.0", Some(&key));
    let check: CheckReport = client.call_as(CHECK, source(&v1)).unwrap();
    assert_eq!(check.candidate.version, "0.1.0");
    let applied: ApplyReport = client.call_as(APPLY, source(&v1)).unwrap();
    assert!(applied.restart_required);
    let v2 = package(&out.path().join("v2"), "0.2.0", Some(&key));
    let applied: ApplyReport = client.call_as(APPLY, source(&v2)).unwrap();
    assert_eq!(applied.from.unwrap().version, "0.1.0");

    let status: StatusResult = client.call_as(STATUS, Value::Null).unwrap();
    assert_eq!(status.status.current.unwrap().version, "0.2.0");
    assert_eq!(status.status.previous.unwrap().version, "0.1.0");
    assert!(status.status.integrity.verified);
    assert_eq!(
        status.status.integrity.signer_key_id.as_deref(),
        Some(key.key_id())
    );
    assert_eq!(
        status.recovery.unwrap().outcome,
        aidex_updater::RecoveryOutcome::Clean
    );

    // Typed refusals carry a stable reason.
    let unsigned = package(&out.path().join("v3"), "0.3.0", None);
    let error = client.call(APPLY, source(&unsigned)).unwrap_err();
    assert_eq!(error.rpc_kind(), Some(ErrorCode::Conflict));
    let aidexd::ClientError::Rpc { data, .. } = &error else {
        panic!("{error}")
    };
    let data = data.clone().unwrap();
    assert_eq!(data["reason"], "unsigned");
    assert_eq!(data["integrity"], true);
    let error = client
        .call(APPLY, json!({"source": v1.to_str().unwrap()}))
        .unwrap_err();
    // `incompatible_version` (-32005) surfaces as the client's incompatibility error.
    assert!(
        matches!(&error, aidexd::ClientError::Incompatible { message, .. } if message.contains("older")),
        "{error:?}"
    );

    let rolled: aidexd::methods::update::RollbackReport =
        client.call_as(ROLLBACK, Value::Null).unwrap();
    assert_eq!(rolled.to.version, "0.1.0");
    let status: StatusResult = client.call_as(STATUS, Value::Null).unwrap();
    assert_eq!(status.status.current.unwrap().version, "0.1.0");

    let events: Value = client
        .call("events.replay", json!({"filter": {"kinds": ["update"]}}))
        .unwrap();
    let names: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["event"]["type"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.contains(&"update.switched".to_owned()), "{names:?}");
    assert!(names.contains(&"update.failed".to_owned()), "{names:?}");
    assert_eq!(names.last().map(String::as_str), Some("update.rolled_back"));
}

#[test]
fn the_daemon_recovers_an_interrupted_update_when_it_starts() {
    let root = common::temp_drive();
    let out = tempfile::tempdir().unwrap();
    let key = KeyPair::generate("daemon-tests").unwrap();
    let paths = DrivePaths::open(root.path()).unwrap();
    let updater = Updater::new(paths.clone(), host(), config(&key));
    let v1 = package(&out.path().join("v1"), "0.1.0", Some(&key));
    updater
        .apply(&UpdateSource::LocalFile(v1), &ApplyOptions::default())
        .unwrap();
    let v2 = package(&out.path().join("v2"), "0.2.0", Some(&key));
    let interrupted = updater.apply_with(
        &UpdateSource::LocalFile(v2.clone()),
        &ApplyOptions::default(),
        &mut |checkpoint| {
            if checkpoint == Checkpoint::VersionCommitted {
                Err(Interrupted)
            } else {
                Ok(())
            }
        },
    );
    assert!(interrupted.is_err());
    drop(updater);

    let mut opts = options();
    opts.updater = Some(config(&key));
    let _daemon = start(root.path(), opts, |_| {});
    let mut client = Client::connect(root.path(), "test-update").unwrap();
    let status: StatusResult = client.call_as(STATUS, Value::Null).unwrap();
    let recovery = status.recovery.unwrap();
    assert_eq!(recovery.outcome, aidex_updater::RecoveryOutcome::RolledBack);
    assert_eq!(status.status.current.unwrap().version, "0.1.0");
    assert!(!status.status.pending);
    assert!(status.status.integrity.verified);
    // The drive accepts the update again.
    let applied: ApplyReport = client.call_as(APPLY, source(&v2)).unwrap();
    assert_eq!(applied.to.version, "0.2.0");
    let events: Value = client
        .call(
            "events.replay",
            json!({"filter": {"kinds": ["update.rolled_back"]}}),
        )
        .unwrap();
    assert_eq!(events["events"].as_array().unwrap().len(), 1);
}
