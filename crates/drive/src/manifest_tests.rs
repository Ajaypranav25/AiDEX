use super::{CURRENT_SCHEMA_VERSION, DriveManifest, ManifestError, PathError, RelativePath};

fn manifest_json(version: u32, runtime_path: &str) -> String {
    serde_json::json!({
        "schema_version": version,
        "layout": {
            "public_runtime": runtime_path,
            "public_models": "models",
            "private_vault": "private/vault",
            "private_identity": "private/identity",
            "private_sessions": "private/sessions",
            "private_config": "private/config"
        }
    })
    .to_string()
}

#[test]
fn current_manifest_round_trips_and_keeps_layout_paths_relative() {
    let manifest = DriveManifest::current();
    let json = manifest.to_json().unwrap();
    let parsed = DriveManifest::from_json(&json).unwrap();

    assert_eq!(parsed, manifest);
    assert_eq!(parsed.schema_version(), CURRENT_SCHEMA_VERSION);
    for path in [
        &parsed.layout.public_runtime,
        &parsed.layout.public_models,
        &parsed.layout.private_vault,
        &parsed.layout.private_identity,
        &parsed.layout.private_sessions,
        &parsed.layout.private_config,
    ] {
        assert!(!path.as_str().starts_with('/'));
        assert!(!path.as_str().contains('\\'));
        assert!(!std::path::Path::new(path.as_str()).is_absolute());
    }
}

#[test]
fn rejects_malformed_and_unsupported_schema_versions_stably() {
    assert_eq!(
        DriveManifest::from_json("{"),
        Err(ManifestError::MalformedJson)
    );
    assert_eq!(
        DriveManifest::from_json("{}"),
        Err(ManifestError::MalformedJson)
    );
    assert_eq!(
        DriveManifest::from_json(&manifest_json(0, "runtime")),
        Err(ManifestError::InvalidManifest)
    );
    assert_eq!(
        DriveManifest::from_json(&manifest_json(u32::MAX, "runtime")),
        Err(ManifestError::UnsupportedVersion(u32::MAX))
    );
    assert_eq!(
        DriveManifest::from_json(r#"{"schema_version":4294967295,"layout":{"future":"shape"}}"#),
        Err(ManifestError::UnsupportedVersion(u32::MAX))
    );
    assert_eq!(
        ManifestError::UnsupportedVersion(2).to_string(),
        "manifest schema version 2 is not supported"
    );
    assert_eq!(
        ManifestError::MalformedJson.to_string(),
        "manifest is not valid JSON"
    );
}

#[test]
fn rejects_invalid_and_empty_manifest_paths() {
    for path in ["", ".", "/", "/absolute", "../escape", "C:\\secrets"] {
        assert!(
            matches!(
                DriveManifest::from_json(&manifest_json(CURRENT_SCHEMA_VERSION, path)),
                Err(ManifestError::InvalidPath(_)),
            ),
            "accepted unsafe path {path:?}"
        );
    }
    assert_eq!(
        DriveManifest::from_json(&manifest_json(CURRENT_SCHEMA_VERSION, "../escape")),
        Err(ManifestError::InvalidPath(PathError::ParentTraversal))
    );
}

#[test]
fn relative_path_deserialization_validates_and_normalizes_boundaries() {
    let parsed: RelativePath = serde_json::from_str(r#""models\\small/./v1""#).unwrap();
    assert_eq!(parsed.as_str(), "models/small/v1");
    assert!(serde_json::from_str::<RelativePath>(r#""../outside""#).is_err());
}
