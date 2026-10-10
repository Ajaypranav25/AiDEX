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
#[cfg(feature = "test-fixture")]
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
}
