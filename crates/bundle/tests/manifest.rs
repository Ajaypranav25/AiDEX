//! `bundle.json` round trip, closed schema, and semantic validation.

mod common;

use aidex_bundle::{BundleManifest, DocumentError, FileRole, MANIFEST_FILE_NAME, Target};
use serde_json::Value;

fn sample() -> (tempfile::TempDir, BundleManifest) {
    let temp = tempfile::tempdir().unwrap();
    let manifest = common::write_bundle(temp.path(), Target::WindowsX64, "0.1.0", "a");
    (temp, manifest)
}

fn as_value(manifest: &BundleManifest) -> Value {
    serde_json::from_slice(&manifest.to_json().unwrap()).unwrap()
}

fn parse(value: &Value) -> Result<BundleManifest, DocumentError> {
    BundleManifest::from_json(value.to_string().as_bytes())
}

#[test]
fn manifest_round_trips_byte_for_byte() {
    let (temp, manifest) = sample();
    let bytes = std::fs::read(temp.path().join(MANIFEST_FILE_NAME)).unwrap();
    let parsed = BundleManifest::from_json(&bytes).unwrap();
    assert_eq!(parsed, manifest);
    assert_eq!(parsed.to_json().unwrap(), bytes);
    assert!(bytes.ends_with(b"}\n"));
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["target"], "windows-x64");
    assert_eq!(value["files"][0]["role"], "launcher");
    assert_eq!(value["state"]["drive_manifest"], "aidex-drive.json");
    assert_eq!(value["state"]["runtime_state"], ".aidex/runtime");
    assert_eq!(value["state"]["layout"]["app"], "app");
    assert_eq!(value["state"]["host_extraction"], false);
    assert_eq!(manifest.file(FileRole::Cli).unwrap().path, "bin/aidex.exe");
    assert_eq!(manifest.critical_files().count(), 4);
}

#[test]
fn unknown_fields_are_rejected_at_every_level() {
    let (_temp, manifest) = sample();
    let base = as_value(&manifest);
    let mut cases = Vec::new();
    let mut top = base.clone();
    top["signature"] = "x".into();
    cases.push(top);
    let mut file = base.clone();
    file["files"][0]["mode"] = 493.into();
    cases.push(file);
    let mut license = base.clone();
    license["licenses"][0]["url"] = "https://example.invalid".into();
    cases.push(license);
    let mut state = base.clone();
    state["state"]["cache_dir"] = "cache".into();
    cases.push(state);
    for case in cases {
        assert!(
            matches!(parse(&case), Err(DocumentError::Malformed(_))),
            "{case}"
        );
    }
    let mut missing = base;
    missing.as_object_mut().unwrap().remove("files");
    assert!(matches!(parse(&missing), Err(DocumentError::Malformed(_))));
}

#[test]
fn newer_or_missing_schema_versions_are_reported_as_unsupported() {
    let (_temp, manifest) = sample();
    let mut newer = as_value(&manifest);
    newer["schema_version"] = 2.into();
    newer["future_field"] = true.into();
    assert_eq!(
        parse(&newer).unwrap_err(),
        DocumentError::UnsupportedSchema {
            found: Some(2),
            supported: 1
        }
    );
    let mut missing = as_value(&manifest);
    missing.as_object_mut().unwrap().remove("schema_version");
    assert!(matches!(
        parse(&missing),
        Err(DocumentError::UnsupportedSchema { found: None, .. })
    ));
    assert!(matches!(
        BundleManifest::from_json(b"[1, 2]"),
        Err(DocumentError::Malformed(_))
    ));
}

#[test]
fn semantic_rules_are_enforced() {
    let (_temp, manifest) = sample();
    let base = as_value(&manifest);
    type Edit = Box<dyn Fn(&mut Value)>;
    let edits: Vec<(&str, Edit)> = vec![
        ("product", Box::new(|v| v["product"] = "other".into())),
        ("version", Box::new(|v| v["version"] = "latest".into())),
        ("commit", Box::new(|v| v["git_commit"] = "HEAD".into())),
        ("target", Box::new(|v| v["target"] = "linux-x64".into())),
        (
            "traversal",
            Box::new(|v| v["files"][4]["path"] = "licenses/../../evil".into()),
        ),
        (
            "absolute",
            Box::new(|v| v["files"][4]["path"] = "/etc/passwd".into()),
        ),
        (
            "digest",
            Box::new(|v| v["files"][0]["sha256"] = "ABC".into()),
        ),
        (
            "role prefix",
            Box::new(|v| v["files"][4]["role"] = "desktop".into()),
        ),
        (
            "not executable",
            Box::new(|v| v["files"][1]["executable"] = false.into()),
        ),
        (
            "empty files",
            Box::new(|v| v["files"] = Value::Array(vec![])),
        ),
        (
            "duplicate",
            Box::new(|v| {
                let mut copy = v["files"][1].clone();
                copy["path"] = copy["path"]
                    .as_str()
                    .unwrap()
                    .to_uppercase()
                    .replace("BIN", "bin")
                    .into();
                v["files"].as_array_mut().unwrap().push(copy);
            }),
        ),
        (
            "license path",
            Box::new(|v| v["licenses"][0]["path"] = "licenses/missing.txt".into()),
        ),
        (
            "drive schema",
            Box::new(|v| v["min_drive_schema"] = 0.into()),
        ),
        (
            "state path",
            Box::new(|v| v["state"]["runtime_state"] = "../runtime".into()),
        ),
    ];
    for (name, edit) in edits {
        let mut value = base.clone();
        edit(&mut value);
        let result = parse(&value);
        assert!(
            matches!(
                result,
                Err(DocumentError::Invalid(_)) | Err(DocumentError::Malformed(_))
            ),
            "{name}: {result:?}"
        );
    }
}

#[test]
fn required_binaries_must_use_target_names() {
    let temp = tempfile::tempdir().unwrap();
    let mut manifest = common::write_bundle(temp.path(), Target::LinuxX64, "0.1.0", "a");
    assert_eq!(manifest.file(FileRole::Cli).unwrap().path, "bin/aidex");
    manifest.validate().unwrap();
    manifest.files.retain(|file| file.role != FileRole::Helper);
    assert!(matches!(
        manifest.validate(),
        Err(DocumentError::Invalid(_))
    ));
}
