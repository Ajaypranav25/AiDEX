use aidex_core::CancelToken;
use aidex_drive::{DriveLayout, DrivePaths};
use aidex_inference::{InferenceService, RuntimeFile, RuntimeManifest};
use aidex_providers::{NoVault, OfflineMode, ProviderRegistry};
use aidex_sandbox::Sandbox;
use std::{fs, sync::Arc};

fn fixture() -> (tempfile::TempDir, InferenceService) {
    let root = tempfile::tempdir().unwrap();
    let paths = DrivePaths::from_layout(root.path().to_path_buf(), DriveLayout::default());
    let gate = Arc::new(OfflineMode::new(true));
    let registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let service = InferenceService::open(paths, registry, gate, Sandbox::locate()).unwrap();
    (root, service)
}

pub fn gguf() -> Vec<u8> {
    // One f32 tensor with a real context metadata key and aligned tensor data.
    let mut b = b"GGUF".to_vec();
    b.extend(3u32.to_le_bytes());
    b.extend(1u64.to_le_bytes());
    b.extend(1u64.to_le_bytes());
    let key = b"qwen2.context_length";
    b.extend((key.len() as u64).to_le_bytes());
    b.extend(key);
    b.extend(4u32.to_le_bytes());
    b.extend(32768u32.to_le_bytes());
    b.extend(1u64.to_le_bytes());
    b.push(b'x');
    b.extend(1u32.to_le_bytes());
    b.extend(1u64.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b.extend(0u64.to_le_bytes());
    b.resize(b.len().div_ceil(32) * 32, 0);
    b.extend(0f32.to_le_bytes());
    b
}

#[test]
fn import_installs_verified_portable_copy_and_preserves_original() {
    let (root, s) = fixture();
    let source = root.path().join("original.gguf");
    fs::write(&source, gguf()).unwrap();
    let m = s
        .import(&source, Some("Tiny Qwen"), &CancelToken::new())
        .unwrap();
    assert_eq!(m.context_window, Some(32768));
    assert_eq!(m.availability.as_str(), "available");
    assert!(m.path.starts_with("models/"));
    assert_eq!(m.sha256.len(), 64);
    assert_eq!(s.catalog().unwrap().models.len(), 1);
    assert!(
        s.import(&source, Some("Different"), &CancelToken::new())
            .is_err()
    );
    s.remove_model(&m.id).unwrap();
    assert!(source.is_file());
    assert!(s.catalog().unwrap().models.is_empty());
}

#[test]
fn bad_gguf_and_cancelled_copy_never_enter_catalog() {
    let (root, s) = fixture();
    let source = root.path().join("bad.gguf");
    fs::write(&source, b"GGUFgarbage").unwrap();
    assert!(s.import(&source, None, &CancelToken::new()).is_err());
    fs::write(&source, gguf()).unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(s.import(&source, None, &cancel).is_err());
    assert!(s.catalog().unwrap().models.is_empty());
}

#[test]
fn catalog_marks_changed_digest_unavailable_and_rejects_unsafe_removal() {
    let (root, s) = fixture();
    let source = root.path().join("tiny.gguf");
    fs::write(&source, gguf()).unwrap();
    let m = s.import(&source, None, &CancelToken::new()).unwrap();
    let managed = root.path().join(&m.path);
    let mut data = gguf();
    *data.last_mut().unwrap() = 1;
    fs::write(managed, data).unwrap();
    assert_eq!(
        s.catalog().unwrap().models[0].availability.as_str(),
        "corrupt"
    );
    assert!(s.remove_model("../original.gguf").is_err());
    assert!(source.exists());
}

#[test]
fn runtime_install_requires_all_checksums_and_rejects_tampered_files() {
    use sha2::{Digest, Sha256};
    let (root, s) = fixture();
    let source = root.path().join("distribution");
    fs::create_dir(&source).unwrap();
    let bytes = b"runtime fixture";
    let executable = format!("llama-server{}", std::env::consts::EXE_SUFFIX);
    fs::write(source.join(&executable), bytes).unwrap();
    let manifest = RuntimeManifest {
        version: 1,
        release: "fixture-v1".into(),
        platform: aidex_drive::platform_runtime_name().unwrap().into(),
        executable: executable.clone(),
        files: vec![RuntimeFile {
            path: executable,
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
        }],
        gpu_supported: false,
    };
    fs::write(
        source.join("runtime-manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let installed = s
        .install_runtime(Some(&source), &CancelToken::new())
        .unwrap();
    assert!(installed.verified);
    assert!(s.catalog().unwrap().runtime.unwrap().verified);
    assert!(
        s.install_runtime(Some(&source), &CancelToken::new())
            .is_err()
    );
    fs::write(
        root.path().join(&installed.path).join(&manifest.executable),
        b"changed",
    )
    .unwrap();
    assert!(!s.catalog().unwrap().runtime.unwrap().verified);
}

#[test]
fn truncated_tensor_payload_is_not_an_installed_model() {
    let (root, s) = fixture();
    let source = root.path().join("short.gguf");
    let mut data = gguf();
    data.truncate(data.len() - 2);
    fs::write(&source, data).unwrap();
    assert!(s.import(&source, None, &CancelToken::new()).is_err());
    assert!(s.catalog().unwrap().models.is_empty());
}

#[test]
fn model_name_collision_is_case_insensitive_and_preserves_both_originals() {
    let (root, s) = fixture();
    let a = root.path().join("a.gguf");
    let b = root.path().join("b.gguf");
    fs::write(&a, gguf()).unwrap();
    let mut data = gguf();
    *data.last_mut().unwrap() = 1;
    fs::write(&b, data).unwrap();
    s.import(&a, Some("Tiny"), &CancelToken::new()).unwrap();
    assert!(s.import(&b, Some("tiny"), &CancelToken::new()).is_err());
    assert!(a.exists() && b.exists());
    assert_eq!(s.catalog().unwrap().models.len(), 1);
}

#[test]
fn published_checksum_sidecar_mismatch_cannot_be_imported() {
    let (root, s) = fixture();
    let source = root.path().join("tiny.gguf");
    fs::write(&source, gguf()).unwrap();
    fs::write(source.with_extension("gguf.sha256"), "0".repeat(64)).unwrap();
    assert!(s.import(&source, None, &CancelToken::new()).is_err());
    assert!(s.catalog().unwrap().models.is_empty());
}

#[test]
fn cancellation_during_copy_removes_staging_without_publishing_install() {
    let (root, s) = fixture();
    let source = root.path().join("large.gguf");
    fs::write(&source, gguf()).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    let token = CancelToken::new();
    let active = token.clone();
    let service = Arc::new(s);
    let worker = service.clone();
    let imported = std::thread::spawn(move || worker.import(&source, None, &active));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while fs::read_dir(root.path().join("models"))
        .unwrap()
        .next()
        .is_none()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    token.cancel();
    assert_eq!(
        imported.join().unwrap().unwrap_err(),
        aidex_inference::InferenceError::Cancelled
    );
    assert!(service.catalog().unwrap().models.is_empty());
    assert_eq!(fs::read_dir(root.path().join("models")).unwrap().count(), 0);
}

#[test]
fn linked_source_parent_cannot_escape_path_validation() {
    let (root, s) = fixture();
    let real = root.path().join("real");
    let link = root.path().join("linked");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("tiny.gguf"), gguf()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();
    #[cfg(windows)]
    {
        let status=std::process::Command::new("powershell").args(["-NoProfile","-NonInteractive","-Command","New-Item -ItemType Junction -Path $env:AIDEX_TEST_LINK -Target $env:AIDEX_TEST_TARGET | Out-Null"])
            .env("AIDEX_TEST_LINK",&link).env("AIDEX_TEST_TARGET",&real).status().unwrap();
        assert!(status.success());
    }
    assert!(
        s.import(&link.join("tiny.gguf"), None, &CancelToken::new())
            .is_err()
    );
    assert!(s.catalog().unwrap().models.is_empty());
    #[cfg(unix)]
    fs::remove_file(&link).unwrap();
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
    assert!(real.join("tiny.gguf").is_file());
}

#[test]
fn unmanaged_originals_in_models_directory_do_not_become_installed_entries() {
    let (root, s) = fixture();
    let source = root.path().join("models").join("original.gguf");
    fs::write(&source, gguf()).unwrap();
    assert!(s.catalog().unwrap().models.is_empty());
    let model = s.import(&source, None, &CancelToken::new()).unwrap();
    s.remove_model(&model.id).unwrap();
    assert!(source.is_file());
    assert!(s.catalog().unwrap().models.is_empty());
}
