use aidex_core::CancelToken;
use aidex_drive::{DriveLayout, DrivePaths};
use aidex_inference::{Backend, InferenceService, StartOptions};
#[cfg(feature = "test-fixture")]
use aidex_inference::{RuntimeFile, RuntimeManifest};
#[cfg(feature = "test-fixture")]
use aidex_providers::{ChatMessage, ChatRequest, ModelId, ProviderId};
use aidex_providers::{NoVault, OfflineMode, ProviderRegistry};
use aidex_sandbox::Sandbox;
#[cfg(feature = "test-fixture")]
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::{fs, time::Duration};

#[test]
fn start_requires_runtime_and_gpu_fails_honestly() {
    let root = tempfile::tempdir().unwrap();
    let gate = Arc::new(OfflineMode::new(true));
    let registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let s = InferenceService::open(
        DrivePaths::from_layout(root.path().to_path_buf(), DriveLayout::default()),
        registry.clone(),
        gate,
        Sandbox::locate(),
    )
    .unwrap();
    let options = StartOptions {
        model_id: format!("m-{}", "0".repeat(64)),
        backend: Backend::Gpu,
        context_window: None,
        threads: None,
    };
    let e = s.start(options, &CancelToken::new()).unwrap_err();
    assert!(e.to_string().contains("GPU"));
    assert!(registry.providers().is_empty());
    assert_eq!(s.status().state, "stopped");
}

#[test]
fn saved_binding_cannot_redirect_inference_outside_the_drive() {
    let root = tempfile::tempdir().unwrap();
    let paths = DrivePaths::from_layout(root.path().to_path_buf(), DriveLayout::default());
    let gate = Arc::new(OfflineMode::new(true));
    let registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let service = Arc::new(
        InferenceService::open(paths.clone(), registry.clone(), gate, Sandbox::locate()).unwrap(),
    );
    let model_id = format!("m-{}", "0".repeat(64));
    fs::write(
        paths.config().join("inference-settings.json"),
        serde_json::to_vec(&serde_json::json!({"model_id":model_id,"backend":"cpu"})).unwrap(),
    )
    .unwrap();
    fs::write(paths.config().join("local-inference.json"), serde_json::to_vec(&serde_json::json!({"version":1,"autoload":true,"settings_file":"inference-settings.json","model_id":model_id,"model_path":"C:/unverified/model.gguf","runtime_path":"runtime/windows-x64/llama-cpp"})).unwrap()).unwrap();
    service.restore_saved().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while service.status().state == "starting" {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(service.status().state, "failed");
    assert!(
        service
            .status()
            .startup_error
            .unwrap()
            .contains("integrity")
    );
    assert!(service.status().pid.is_none());
    assert!(registry.providers().is_empty());
    service.shutdown().unwrap();
}

#[cfg(feature = "test-fixture")]
#[test]
fn healthy_process_registers_local_provider_and_tears_down() {
    let root = tempfile::tempdir().unwrap();
    let gate = Arc::new(OfflineMode::new(true));
    let registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let s = Arc::new(
        InferenceService::open(
            DrivePaths::from_layout(root.path().to_path_buf(), DriveLayout::default()),
            registry.clone(),
            gate,
            Sandbox::locate(),
        )
        .unwrap(),
    );
    let source = root.path().join("tiny.gguf");
    let mut gguf = b"GGUF".to_vec();
    gguf.extend(3u32.to_le_bytes());
    gguf.extend(1u64.to_le_bytes());
    gguf.extend(0u64.to_le_bytes());
    gguf.extend(1u64.to_le_bytes());
    gguf.push(b'x');
    gguf.extend(1u32.to_le_bytes());
    gguf.extend(1u64.to_le_bytes());
    gguf.extend(0u32.to_le_bytes());
    gguf.extend(0u64.to_le_bytes());
    gguf.resize(gguf.len().div_ceil(32) * 32, 0);
    gguf.extend(0f32.to_le_bytes());
    fs::write(&source, gguf).unwrap();
    let m = s
        .import(&source, Some("Tiny coding model"), &CancelToken::new())
        .unwrap();
    let distribution = root.path().join("distribution");
    fs::create_dir(&distribution).unwrap();
    let exe = format!("llama-server{}", std::env::consts::EXE_SUFFIX);
    let bytes = fs::read(env!("CARGO_BIN_EXE_aidex-inference-fixture")).unwrap();
    fs::write(distribution.join(&exe), &bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(distribution.join(&exe), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let manifest = RuntimeManifest {
        version: 1,
        release: "fixture-v1".into(),
        platform: aidex_drive::platform_runtime_name().unwrap().into(),
        executable: exe.clone(),
        files: vec![RuntimeFile {
            path: exe,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            size_bytes: bytes.len() as u64,
        }],
        gpu_supported: false,
    };
    fs::write(
        distribution.join("runtime-manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    s.install_runtime(Some(&distribution), &CancelToken::new())
        .unwrap();
    let options = StartOptions {
        model_id: m.id.clone(),
        backend: Backend::Auto,
        context_window: Some(2048),
        threads: Some(2),
    };
    let worker = s.clone();
    let option_clone = options.clone();
    let start = std::thread::spawn(move || worker.start(option_clone, &CancelToken::new()));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while s.status().state != "starting" {
        assert!(
            std::time::Instant::now() < deadline,
            "start did not enter loading state"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(registry.providers().is_empty());
    assert!(s.start(options.clone(), &CancelToken::new()).is_err());
    let ready = start.join().unwrap().unwrap();
    assert_eq!(ready.state, "ready");
    assert_eq!(ready.backend, Some(Backend::Cpu));
    assert!(!ready.controls.is_empty());
    let provider = registry
        .provider(&ProviderId::new("local-llama").unwrap())
        .unwrap();
    assert_eq!(provider.models().unwrap().len(), 1);
    assert_eq!(
        provider.models().unwrap()[0].display_name,
        "Tiny coding model"
    );
    let response = provider
        .chat(
            &ChatRequest::new(
                ModelId::new(m.id.clone()).unwrap(),
                vec![ChatMessage::user("hi")],
            ),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(response.message.content, "local works");
    assert!(s.remove_model(&m.id).is_err());
    let endpoint = ready.endpoint.unwrap();
    s.stop().unwrap();
    assert!(registry.providers().is_empty());
    assert!(provider.models().is_err());
    assert!(
        std::net::TcpStream::connect(
            endpoint
                .trim_start_matches("http://")
                .trim_end_matches("/v1")
        )
        .is_err()
    );
    s.start(options.clone(), &CancelToken::new()).unwrap();
    assert_eq!(s.catalog().unwrap().settings.unwrap().threads, Some(2));
    assert_eq!(
        s.catalog().unwrap().settings.unwrap().backend,
        Backend::Auto
    );
    s.stop().unwrap();
    let worker = s.clone();
    let mut cancelled_options = options.clone();
    cancelled_options.context_window = Some(512);
    let cancelled =
        std::thread::spawn(move || worker.start(cancelled_options, &CancelToken::new()));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while s.status().pid.is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(registry.providers().is_empty());
    s.stop().unwrap();
    assert_eq!(
        cancelled.join().unwrap().unwrap_err(),
        aidex_inference::InferenceError::Cancelled
    );
    assert!(registry.providers().is_empty());
    let mut exited = options;
    exited.context_window = Some(1024);
    assert!(s.start(exited, &CancelToken::new()).is_err());
    assert_eq!(s.status().state, "stopped");
    assert!(registry.providers().is_empty());
    // Saved IDs and relative manifest paths survive relocation without another import.
    s.shutdown().unwrap();
    let relocated = tempfile::tempdir().unwrap();
    fn copy_tree(source: &std::path::Path, destination: &std::path::Path) {
        fs::create_dir_all(destination).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &destination.join(entry.file_name()));
            } else {
                fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
            }
        }
    }
    copy_tree(root.path(), relocated.path());
    let gate = Arc::new(OfflineMode::new(true));
    let restored_registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let restored = Arc::new(
        InferenceService::open(
            DrivePaths::from_layout(relocated.path().to_path_buf(), DriveLayout::default()),
            restored_registry.clone(),
            gate,
            Sandbox::locate(),
        )
        .unwrap(),
    );
    assert!(restored.restore_saved().unwrap());
    assert_eq!(restored.status().state, "starting");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while restored.status().state == "starting" {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        restored.status().state,
        "ready",
        "{:?}",
        restored.status().startup_error
    );
    assert_eq!(
        restored.status().model_name.as_deref(),
        Some("Tiny coding model")
    );
    assert_eq!(restored_registry.providers().len(), 1);
    let paths = DrivePaths::from_layout(relocated.path().to_path_buf(), DriveLayout::default());
    let binding: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.config().join("local-inference.json")).unwrap())
            .unwrap();
    assert_eq!(binding["model_path"], format!("models/{}/model.gguf", m.id));
    assert_eq!(
        binding["runtime_path"],
        format!(
            "runtime/{}/llama-cpp",
            aidex_drive::platform_runtime_name().unwrap()
        )
    );
    assert_eq!(binding["settings_file"], "inference-settings.json");
    assert_eq!(binding["autoload"], true);
    assert!(!binding.to_string().contains(root.path().to_str().unwrap()));
    restored.shutdown().unwrap();
    assert!(restored.restore_saved().is_err());
    assert_eq!(restored.status().state, "stopped");

    // Cancel a queued restore before it can start or publish a process.
    let gate = Arc::new(OfflineMode::new(true));
    let registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let cancelled = Arc::new(
        InferenceService::open(
            DrivePaths::from_layout(relocated.path().to_path_buf(), DriveLayout::default()),
            registry.clone(),
            gate,
            Sandbox::locate(),
        )
        .unwrap(),
    );
    cancelled.restore_saved().unwrap();
    cancelled.shutdown().unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(cancelled.status().state, "stopped");
    assert!(cancelled.status().pid.is_none());
    assert!(registry.providers().is_empty());
    let binding_path = paths.config().join("local-inference.json");
    let mut binding = binding;
    binding["autoload"] = serde_json::json!(false);
    fs::write(&binding_path, serde_json::to_vec(&binding).unwrap()).unwrap();
    let gate = Arc::new(OfflineMode::new(true));
    let disabled_registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let disabled = Arc::new(
        InferenceService::open(
            paths.clone(),
            disabled_registry.clone(),
            gate,
            Sandbox::locate(),
        )
        .unwrap(),
    );
    disabled.restore_saved().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while disabled.status().state == "starting" {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(disabled.status().state, "stopped");
    assert!(disabled_registry.providers().is_empty());
    disabled.shutdown().unwrap();
    binding["autoload"] = serde_json::json!(true);
    fs::write(&binding_path, serde_json::to_vec(&binding).unwrap()).unwrap();
    // Stopping during background loading tears down its child and permits retry.
    let settings_path = paths.config().join("inference-settings.json");
    let normal_settings: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    let mut slow_settings = normal_settings.clone();
    slow_settings["context_window"] = serde_json::json!(512);
    fs::write(&settings_path, serde_json::to_vec(&slow_settings).unwrap()).unwrap();
    let gate = Arc::new(OfflineMode::new(true));
    let slow_registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let slow = Arc::new(
        InferenceService::open(
            paths.clone(),
            slow_registry.clone(),
            gate,
            Sandbox::locate(),
        )
        .unwrap(),
    );
    slow.restore_saved().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while slow.status().pid.is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    slow.stop().unwrap();
    assert_eq!(slow.status().state, "stopped");
    assert!(slow.status().pid.is_none());
    assert!(slow_registry.providers().is_empty());
    slow.start(
        serde_json::from_value(normal_settings).unwrap(),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(slow.status().state, "ready");
    slow.shutdown().unwrap();
    fs::write(
        relocated
            .path()
            .join("models")
            .join(&m.id)
            .join("model.gguf"),
        b"tampered",
    )
    .unwrap();
    let gate = Arc::new(OfflineMode::new(true));
    let failed_registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let failed = Arc::new(
        InferenceService::open(
            DrivePaths::from_layout(relocated.path().to_path_buf(), DriveLayout::default()),
            failed_registry.clone(),
            gate,
            Sandbox::locate(),
        )
        .unwrap(),
    );
    failed.restore_saved().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while failed.status().state == "starting" {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(failed.status().state, "failed");
    assert!(failed.status().startup_error.unwrap().contains("integrity"));
    assert_eq!(failed.status().model_id.as_deref(), Some(m.id.as_str()));
    assert!(failed.status().pid.is_none());
    assert!(failed_registry.providers().is_empty());
    failed.shutdown().unwrap();
}
