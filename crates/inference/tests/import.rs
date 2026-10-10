//! Model import, registry, and verification: models are hashed and checked at
//! import, never overwrite existing assets, record licence and source, survive
//! interruption, and are verified before use.

mod support;

use std::{
    fs,
    time::{Duration, SystemTime},
};

use aidex_core::CancelToken;
use aidex_inference::{
    ImportOptions, ImportPhase, InferenceError, LocalModelId, ModelRegistry, STAGING_DIR,
    VerificationMethod, gguf::GgufBuilder, import_model, import_model_with, remove_stale_partials,
    verify_model, verify_model_with,
};
use support::{fake_model, sha256_file, temp_drive, write_model};

fn options(id: &str) -> ImportOptions {
    ImportOptions {
        id: Some(id.into()),
        ..ImportOptions::default()
    }
}

#[test]
fn import_registers_the_model_with_licence_and_source() {
    let (_root, paths) = temp_drive();
    let source_dir = tempfile::tempdir().unwrap();
    let source = write_model(
        source_dir.path(),
        "Tiny-Coder-Q4_K_M",
        fake_model("Tiny Coder"),
    );
    let license = source_dir.path().join("LICENSE");
    fs::write(&license, "Apache License 2.0 (fixture)").unwrap();
    let mut phases = Vec::new();
    let entry = import_model_with(
        &paths,
        &source,
        &ImportOptions {
            license: Some("Apache-2.0".into()),
            license_file: Some(license),
            source_url: Some("https://huggingface.co/example/tiny-coder-gguf".into()),
            source_revision: Some("0123abcd".into()),
            ..ImportOptions::default()
        },
        &mut |progress| phases.push(progress.phase),
        &CancelToken::new(),
    )
    .unwrap();

    assert_eq!(
        entry.id.as_str(),
        "tiny-coder-q4_k_m",
        "derived from the file name"
    );
    assert_eq!(entry.file, "models/tiny-coder-q4_k_m/model.gguf");
    assert_eq!(entry.sha256, sha256_file(&source));
    assert_eq!(entry.size_bytes, fs::metadata(&source).unwrap().len());
    assert_eq!(entry.architecture, "qwen2");
    assert_eq!(entry.family, "qwen");
    assert_eq!(entry.quantization.as_deref(), Some("Q4_K_M"));
    assert_eq!(entry.context_length, 4096);
    assert!(entry.capabilities.chat && entry.capabilities.code && entry.capabilities.json_schema);
    assert_eq!(entry.capabilities.tool_calls.as_str(), "structured");
    assert_eq!(entry.license.name, "Apache-2.0");
    assert_eq!(entry.license.origin, "user");
    assert_eq!(
        entry.license.file.as_deref(),
        Some("models/tiny-coder-q4_k_m/LICENSE")
    );
    assert_eq!(
        entry.source.url.as_deref(),
        Some("https://huggingface.co/example/tiny-coder-gguf")
    );
    assert_eq!(entry.source.revision.as_deref(), Some("0123abcd"));
    assert_eq!(entry.source.original_file_name, "Tiny-Coder-Q4_K_M.gguf");
    for phase in [
        ImportPhase::Inspecting,
        ImportPhase::Copying,
        ImportPhase::Verifying,
        ImportPhase::Registering,
    ] {
        assert!(phases.contains(&phase), "{phase:?} not reported");
    }

    let model_path = entry.model_path(&paths).unwrap();
    assert_eq!(sha256_file(&model_path), entry.sha256);
    assert_eq!(
        fs::read_to_string(paths.models().join("tiny-coder-q4_k_m/LICENSE")).unwrap(),
        "Apache License 2.0 (fixture)"
    );
    let registry = ModelRegistry::open(&paths).unwrap();
    assert_eq!(registry.get(&entry.id).unwrap(), entry, "TOML round trip");
    let text = fs::read_to_string(registry.entry_path(&entry.id)).unwrap();
    assert!(text.contains("version = 1") && text.contains("format = \"gguf\""));
    assert!(
        !text.contains(&source_dir.path().display().to_string()),
        "no host paths recorded"
    );
    assert!(
        fs::read_dir(paths.models().join(STAGING_DIR))
            .unwrap()
            .next()
            .is_none(),
        "no partial copies remain"
    );
}

#[test]
fn gguf_licence_is_recorded_when_none_is_given_and_unknown_otherwise() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let with = write_model(dir.path(), "with", fake_model("With"));
    let entry = import_model(&paths, &with, &options("with")).unwrap();
    assert_eq!(
        (entry.license.name.as_str(), entry.license.origin.as_str()),
        ("apache-2.0", "gguf")
    );
    let without = write_model(
        dir.path(),
        "without",
        fake_model("Without").without("general.license"),
    );
    let entry = import_model(&paths, &without, &options("without")).unwrap();
    assert_eq!(
        (entry.license.name.as_str(), entry.license.origin.as_str()),
        ("unknown", "unknown")
    );
}

#[test]
fn import_never_overwrites_existing_models() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let first = write_model(dir.path(), "first", fake_model("First"));
    let entry = import_model(&paths, &first, &options("shared")).unwrap();
    let before = sha256_file(&entry.model_path(&paths).unwrap());

    let second = write_model(dir.path(), "second", fake_model("Second"));
    let error = import_model(&paths, &second, &options("shared")).unwrap_err();
    assert_eq!(error.kind(), "already_exists", "{error}");
    assert!(error.advice().unwrap().contains("--id"));
    assert_eq!(sha256_file(&entry.model_path(&paths).unwrap()), before);
    assert_eq!(
        ModelRegistry::open(&paths)
            .unwrap()
            .get(&entry.id)
            .unwrap()
            .display_name,
        "First"
    );

    // An unregistered directory (for example a crash after the move) also blocks the id.
    fs::create_dir_all(paths.models().join("orphan")).unwrap();
    fs::write(paths.models().join("orphan/model.gguf"), b"user data").unwrap();
    let error = import_model(&paths, &second, &options("orphan")).unwrap_err();
    assert_eq!(error.kind(), "already_exists");
    assert_eq!(
        fs::read(paths.models().join("orphan/model.gguf")).unwrap(),
        b"user data"
    );
}

#[test]
fn interrupted_imports_leave_only_recoverable_partials() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let source = write_model(
        dir.path(),
        "big",
        fake_model("Big").tensor("blk.0.ffn.weight", &[1024, 1024], 1),
    );

    // Cancellation mid-copy removes the partial and registers nothing.
    let cancel = CancelToken::new();
    let error = import_model_with(
        &paths,
        &source,
        &options("big"),
        &mut |progress| {
            if progress.phase == ImportPhase::Copying && progress.done_bytes > 0 {
                cancel.cancel();
            }
        },
        &cancel,
    )
    .unwrap_err();
    assert_eq!(error, InferenceError::Cancelled);
    let staging = paths.models().join(STAGING_DIR);
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
    assert!(!paths.models().join("big").exists());
    assert!(
        !ModelRegistry::open(&paths)
            .unwrap()
            .contains(&LocalModelId::new("big").unwrap())
    );

    // A crash leaves a `.partial`; the next import removes it once it is stale
    // and keeps one that may belong to an import still running.
    let stale = staging.join("big-0000000000000000.partial");
    fs::write(&stale, b"half a model").unwrap();
    let old = SystemTime::now() - Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(&stale)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let fresh = staging.join("other-1111111111111111.partial");
    fs::write(&fresh, b"in progress").unwrap();
    import_model(&paths, &source, &options("big")).unwrap();
    assert!(!stale.exists(), "stale partial removed");
    assert!(fresh.exists(), "a recent partial is left alone");
    assert_eq!(
        remove_stale_partials(&staging, Duration::ZERO),
        vec![fresh.clone()]
    );
}

#[test]
fn incompatible_and_malformed_files_are_refused_without_side_effects() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let not_gguf = dir.path().join("notes.gguf");
    fs::write(&not_gguf, b"this is not a model").unwrap();
    assert_eq!(
        import_model(&paths, &not_gguf, &ImportOptions::default())
            .unwrap_err()
            .kind(),
        "invalid_gguf"
    );
    let projector = write_model(
        dir.path(),
        "mmproj",
        GgufBuilder::new("clip").tensor("v", &[32], 0),
    );
    let error = import_model(&paths, &projector, &ImportOptions::default()).unwrap_err();
    assert_eq!(error.kind(), "incompatible_model");
    assert!(error.to_string().contains("clip"));

    let good = write_model(dir.path(), "good", fake_model("Good"));
    let bytes = fs::read(&good).unwrap();
    let truncated = dir.path().join("truncated.gguf");
    fs::write(&truncated, &bytes[..bytes.len() - 64]).unwrap();
    assert_eq!(
        import_model(&paths, &truncated, &ImportOptions::default())
            .unwrap_err()
            .kind(),
        "invalid_gguf"
    );

    let error = import_model(
        &paths,
        &good,
        &ImportOptions {
            expected_sha256: Some("0".repeat(64)),
            ..options("good")
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), "hash_mismatch");
    for error in [
        import_model(&paths, &good, &options("Bad Id")).unwrap_err(),
        import_model(
            &paths,
            &good,
            &ImportOptions {
                source_url: Some("https://user:pass@example.com/model".into()),
                ..options("good")
            },
        )
        .unwrap_err(),
        import_model(
            &paths,
            &good,
            &ImportOptions {
                source_url: Some("ftp://example.com/model".into()),
                ..options("good")
            },
        )
        .unwrap_err(),
    ] {
        assert!(
            matches!(error.kind(), "invalid_model_id" | "invalid_argument"),
            "{error}"
        );
    }
    let listing = ModelRegistry::open(&paths).unwrap().list().unwrap();
    assert!(listing.entries.is_empty() && listing.problems.is_empty());
    assert_eq!(
        fs::read_dir(paths.models()).unwrap().count(),
        2,
        "only registry/ and .staging/ exist"
    );
}

#[test]
fn verification_caches_success_and_refuses_tampered_models() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let source = write_model(dir.path(), "m", fake_model("M"));
    let entry = import_model(&paths, &source, &options("m")).unwrap();

    let first = verify_model(&paths, &entry).unwrap();
    assert_eq!(
        first.method,
        VerificationMethod::Cached,
        "import hashed the copy"
    );
    let forced = verify_model_with(&paths, &entry, true, &mut |_| {}, &CancelToken::new()).unwrap();
    assert_eq!(forced.method, VerificationMethod::FullHash);
    assert_eq!(forced.sha256, entry.sha256);

    // Same size, different bytes, new modification time: rehashed and refused.
    let path = entry.model_path(&paths).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&path, &bytes).unwrap();
    let later = SystemTime::now() + Duration::from_secs(5);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let error = verify_model(&paths, &entry).unwrap_err();
    let InferenceError::HashMismatch { expected, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(expected, &entry.sha256);
    assert!(error.to_string().contains("refusing to use it"));
    assert!(error.advice().unwrap().contains("import it again"));
    // The failure is not cached: a forced check fails too.
    assert_eq!(
        verify_model_with(&paths, &entry, true, &mut |_| {}, &CancelToken::new())
            .unwrap_err()
            .kind(),
        "hash_mismatch"
    );

    // A truncated or missing file is refused before hashing.
    fs::write(&path, &bytes[..10]).unwrap();
    assert_eq!(
        verify_model(&paths, &entry).unwrap_err().kind(),
        "hash_mismatch"
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(
        verify_model(&paths, &entry).unwrap_err().kind(),
        "missing_file"
    );
}

#[test]
fn registry_reports_corrupt_entries_next_to_good_ones() {
    let (_root, paths) = temp_drive();
    let dir = tempfile::tempdir().unwrap();
    let source = write_model(dir.path(), "good", fake_model("Good"));
    import_model(&paths, &source, &options("good")).unwrap();
    let registry = ModelRegistry::open(&paths).unwrap();
    fs::write(
        registry.dir().join("broken.toml"),
        "version = 1\napi_key = \"sk-123\"\n",
    )
    .unwrap();
    fs::write(registry.dir().join("notes.txt"), "ignored").unwrap();
    let listing = registry.list().unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.problems.len(), 1);
    assert_eq!(listing.problems[0].file, "broken.toml");
    assert!(
        !listing.problems[0].message.contains("sk-123"),
        "values are not echoed"
    );
    let missing = LocalModelId::new("missing").unwrap();
    let error = registry.get(&missing).unwrap_err();
    assert_eq!(error.kind(), "model_not_found");
    assert!(error.advice().unwrap().contains("aidex model import"));
}
