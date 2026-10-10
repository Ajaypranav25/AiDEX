use super::{
    CURRENT_SCHEMA_VERSION, DriveManifest, ManifestError, PathError, RelativePath, StoredManifest,
    V1_SCHEMA_VERSION,
};
use crate::{DriveLayout, DriveLayoutV1};

const V2_FIELDS: [(&str, &str); 19] = [
    ("public_runtime", "runtime"),
    ("public_models", "models"),
    ("private_vault", "private/vault"),
    ("private_identity", "private/identity"),
    ("private_sessions", "private/sessions"),
    ("private_config", "private/config"),
    ("public_agents", "agents"),
    ("public_skills", "skills"),
    ("public_mcp", "mcp"),
    ("public_plugins", "plugins"),
    ("public_policies", "policies"),
    ("workspaces", "workspaces"),
    ("cache", "cache"),
    ("tmp", "tmp"),
    ("logs", "logs"),
    ("app", "app"),
    ("manifests", "manifests"),
    ("private_memory", "private/memory"),
    ("private_index", "private/index"),
];

/// A complete v2 manifest with one field overridden.
fn v2_json_with(field: &str, value: &str) -> String {
    let mut layout = serde_json::Map::new();
    for (name, default) in V2_FIELDS {
        let path = if name == field { value } else { default };
        layout.insert(name.into(), path.into());
    }
    serde_json::json!({ "schema_version": 2, "layout": layout }).to_string()
}

fn manifest_json(version: u32, runtime_path: &str) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(&v2_json_with("public_runtime", runtime_path)).unwrap();
    value["schema_version"] = version.into();
    value.to_string()
}

fn v1_json(runtime_path: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
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
    assert_eq!(CURRENT_SCHEMA_VERSION, 2);
    assert_eq!(parsed.layout.entries().count(), V2_FIELDS.len());
    for (_, path) in parsed.layout.entries() {
        assert!(!path.as_str().starts_with('/'));
        assert!(!path.as_str().contains('\\'));
        assert!(!std::path::Path::new(path.as_str()).is_absolute());
    }
}

#[test]
fn v2_defaults_match_the_documented_layout_and_json_field_names() {
    let defaults = DriveLayout::default();
    let entries: Vec<_> = defaults
        .entries()
        .map(|(name, path)| (name, path.as_str()))
        .collect();
    assert_eq!(entries, V2_FIELDS);

    let encoded: serde_json::Value =
        serde_json::from_str(&DriveManifest::current().to_json().unwrap()).unwrap();
    assert_eq!(encoded["schema_version"], 2);
    for (name, path) in V2_FIELDS {
        assert_eq!(encoded["layout"][name], path, "field {name}");
    }
    assert_eq!(
        encoded["layout"].as_object().unwrap().len(),
        V2_FIELDS.len()
    );
    assert_eq!(
        StoredManifest::from_json(&v2_json_with("cache", "cache")).unwrap(),
        StoredManifest::V2(DriveManifest::current())
    );
}

#[test]
fn v1_manifests_still_parse_but_require_migration_for_current_readers() {
    let stored = StoredManifest::from_json(&v1_json("bin/runtime")).unwrap();
    assert_eq!(stored.schema_version(), V1_SCHEMA_VERSION);
    let StoredManifest::V1(legacy) = &stored else {
        panic!("expected a v1 manifest, got {stored:?}");
    };
    assert_eq!(legacy.layout.public_runtime.as_str(), "bin/runtime");
    assert_eq!(
        DriveManifest::from_json(&v1_json("runtime")),
        Err(ManifestError::MigrationRequired(1))
    );

    let migrated = legacy.migrate_to_v2().unwrap();
    assert_eq!(migrated.schema_version(), 2);
    assert_eq!(migrated.layout.public_runtime.as_str(), "bin/runtime");
    assert_eq!(migrated.layout.private_config.as_str(), "private/config");
    assert_eq!(migrated.layout.private_memory.as_str(), "private/memory");
    assert_eq!(migrated.layout.workspaces.as_str(), "workspaces");
    assert_eq!(
        DriveLayoutV1::default().with_v2_defaults(),
        DriveLayout::default()
    );
}

#[test]
fn schemas_are_closed_for_both_versions() {
    let mut v1: serde_json::Value = serde_json::from_str(&v1_json("runtime")).unwrap();
    v1["layout"]["cache"] = "cache".into();
    assert_eq!(
        StoredManifest::from_json(&v1.to_string()),
        Err(ManifestError::InvalidManifest),
        "v1 must not accept v2-only fields"
    );

    let mut v2: serde_json::Value = serde_json::from_str(&v2_json_with("cache", "cache")).unwrap();
    v2["layout"]["extra"] = "extra".into();
    assert_eq!(
        StoredManifest::from_json(&v2.to_string()),
        Err(ManifestError::InvalidManifest)
    );
    v2["layout"].as_object_mut().unwrap().remove("extra");
    v2["layout"].as_object_mut().unwrap().remove("logs");
    assert_eq!(
        StoredManifest::from_json(&v2.to_string()),
        Err(ManifestError::InvalidManifest),
        "v2 fields are all required"
    );
    let mut top: serde_json::Value = serde_json::from_str(&v2_json_with("cache", "cache")).unwrap();
    top["comment"] = "extra".into();
    assert_eq!(
        StoredManifest::from_json(&top.to_string()),
        Err(ManifestError::MalformedJson)
    );
    assert_eq!(
        StoredManifest::from_json(&v2_json_with("cache", "")),
        Err(ManifestError::InvalidPath(PathError::Empty))
    );
    let mut wrong_type: serde_json::Value =
        serde_json::from_str(&v2_json_with("cache", "cache")).unwrap();
    wrong_type["layout"]["cache"] = 7.into();
    assert_eq!(
        StoredManifest::from_json(&wrong_type.to_string()),
        Err(ManifestError::InvalidManifest)
    );
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
        DriveManifest::from_json(&manifest_json(3, "runtime")),
        Err(ManifestError::UnsupportedVersion(3))
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
fn rejects_traversal_and_absolute_paths_in_every_v2_field() {
    for (field, _) in V2_FIELDS {
        for path in [
            "../escape",
            "cache/../../escape",
            "/absolute",
            "C:\\secrets",
            "\\\\server\\share",
        ] {
            assert!(
                matches!(
                    StoredManifest::from_json(&v2_json_with(field, path)),
                    Err(ManifestError::InvalidPath(_))
                ),
                "{field} accepted unsafe path {path:?}"
            );
        }
    }
}

#[test]
fn rejects_duplicate_and_reserved_layout_paths() {
    assert_eq!(
        StoredManifest::from_json(&v2_json_with("cache", "tmp")),
        Err(ManifestError::DuplicatePath {
            first: "cache",
            second: "tmp"
        })
    );
    assert_eq!(
        StoredManifest::from_json(&v2_json_with("private_index", "private/vault")),
        Err(ManifestError::DuplicatePath {
            first: "private_vault",
            second: "private_index"
        })
    );
    for alias in ["Logs", "LOGS/", "./logs", "logs.", "logs "] {
        assert!(
            matches!(
                StoredManifest::from_json(&v2_json_with("app", alias)),
                Err(ManifestError::DuplicatePath { .. })
            ),
            "{alias:?} should collide with `logs` on case-insensitive filesystems"
        );
    }
    for reserved in [
        ".aidex",
        ".aidex/runtime",
        "aidex-drive.json",
        "AIDEX-DRIVE.V1.JSON",
        "aidex-drive.json.v2-tmp",
    ] {
        assert!(
            matches!(
                StoredManifest::from_json(&v2_json_with("logs", reserved)),
                Err(ManifestError::ReservedPath("logs"))
            ),
            "accepted reserved path {reserved:?}"
        );
    }
    assert!(
        StoredManifest::from_json(&v2_json_with("cache", "private/cache")).is_ok(),
        "nesting beneath another declared directory's parent is allowed"
    );
    assert!(
        StoredManifest::from_json(&v2_json_with("private_index", "private/vault/index")).is_ok(),
        "nesting beneath another declared directory is allowed"
    );

    let layout = DriveLayout {
        tmp: RelativePath::new("cache").unwrap(),
        ..DriveLayout::default()
    };
    assert!(matches!(
        DriveManifest::new(layout).to_json(),
        Err(ManifestError::DuplicatePath { .. })
    ));
}

#[test]
fn v1_migration_rejects_layouts_that_collide_with_new_defaults() {
    let StoredManifest::V1(legacy) = StoredManifest::from_json(&v1_json("cache")).unwrap() else {
        panic!("expected v1");
    };
    assert_eq!(
        legacy.migrate_to_v2(),
        Err(ManifestError::DuplicatePath {
            first: "public_runtime",
            second: "cache"
        })
    );
}

#[test]
fn relative_path_deserialization_validates_and_normalizes_boundaries() {
    let parsed: RelativePath = serde_json::from_str(r#""models\\small/./v1""#).unwrap();
    assert_eq!(parsed.as_str(), "models/small/v1");
    assert!(serde_json::from_str::<RelativePath>(r#""../outside""#).is_err());
}
