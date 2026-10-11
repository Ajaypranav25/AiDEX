//! Real llama.cpp validation (ignored by default). Run with the staged assets:
//!
//! ```text
//! cargo build -p aidex-sandbox --bin aidex-sandbox-exec
//! AIDEX_TEST_LLAMA_ZIP=<llama-b11205-bin-win-cpu-x64.zip> \
//! AIDEX_TEST_GGUF=<qwen2.5-coder-0.5b-instruct-q4_k_m.gguf> \
//! cargo test -p aidex-inference --test real_llama -- --ignored --nocapture
//! ```
//!
//! The upstream archive is checked against the llama.cpp b11205 release digest,
//! extracted into a flat distribution with a `runtime-manifest.json` built from
//! its files, installed and imported into a temporary drive, started with
//! host-aware `auto` settings (after the opt-in GPU probe), and asked for a
//! completion and a JSON-schema answer with cloud networking offline.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use aidex_core::CancelToken;
use aidex_drive::{DriveLayout, DrivePaths};
use aidex_inference::{
    Backend, InferenceService, LOCAL_PROVIDER_ID, RuntimeFile, RuntimeManifest, StartOptions,
};
use aidex_providers::{
    ChatMessage, ChatRequest, ModelId, NoVault, OfflineMode, ProviderId, ProviderRegistry,
    ResponseFormat, StreamEvent,
};
use aidex_sandbox::Sandbox;
use sha2::{Digest, Sha256};

/// SHA-256 of `llama-b11205-bin-win-cpu-x64.zip` from the GitHub release digest.
const PINNED_WIN_CPU_X64: &str = "2a8bf1ff808e8f6d2329f50f7005419a9930d9ecfbeae5047cfff475e87c2db0";

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(
        std::env::var_os(name)
            .unwrap_or_else(|| panic!("set {name} to run the real llama.cpp validation")),
    )
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Extract a flat release zip and describe every file in `runtime-manifest.json`.
fn distribution(archive: &Path, into: &Path) -> usize {
    let bytes = fs::read(archive).unwrap();
    if std::env::consts::OS == "windows" && std::env::consts::ARCH == "x86_64" {
        assert_eq!(
            sha256(&bytes),
            PINNED_WIN_CPU_X64,
            "the archive is not the pinned b11205 release"
        );
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut files = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).unwrap();
        let name = entry.name().to_owned();
        assert!(
            !name.contains(['/', '\\', ':']) && !name.starts_with('.'),
            "unexpected archive layout: {name}"
        );
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        fs::write(into.join(&name), &content).unwrap();
        files.push(RuntimeFile {
            path: name,
            sha256: sha256(&content),
            size_bytes: content.len() as u64,
        });
    }
    let count = files.len();
    let manifest = RuntimeManifest {
        version: 1,
        release: "b11205".into(),
        platform: aidex_drive::platform_runtime_name().unwrap().into(),
        executable: format!("llama-server{}", std::env::consts::EXE_SUFFIX),
        files,
        gpu_supported: false,
    };
    fs::write(
        into.join("runtime-manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    count
}

#[test]
#[ignore = "needs AIDEX_TEST_LLAMA_ZIP and AIDEX_TEST_GGUF (real llama.cpp runtime and model)"]
fn real_runtime_runs_the_staged_model_offline() {
    let archive = env_path("AIDEX_TEST_LLAMA_ZIP");
    let gguf = env_path("AIDEX_TEST_GGUF");
    let drive = tempfile::tempdir().unwrap();
    let staged = tempfile::tempdir().unwrap();
    let gate = Arc::new(OfflineMode::new(true));
    let registry = Arc::new(ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        gate.clone(),
    ));
    let service = InferenceService::open(
        DrivePaths::from_layout(drive.path().to_path_buf(), DriveLayout::default()),
        registry.clone(),
        gate,
        Sandbox::locate(),
    )
    .unwrap();
    let cancel = CancelToken::new();

    let started = Instant::now();
    let files = distribution(&archive, staged.path());
    let runtime = service
        .install_runtime(Some(staged.path()), &cancel)
        .unwrap();
    println!(
        "runtime: llama.cpp {} ({files} files) installed at {} in {:?}",
        runtime.release,
        runtime.path,
        started.elapsed()
    );
    assert!(runtime.verified);

    let started = Instant::now();
    let model = service
        .import(&gguf, Some("Qwen2.5 Coder 0.5B Instruct Q4_K_M"), &cancel)
        .unwrap();
    println!(
        "model: imported {} ({} MiB, GGUF v{}, context {:?}) in {:?}",
        model.id,
        model.size_bytes >> 20,
        model.gguf_version,
        model.context_window,
        started.elapsed()
    );

    let host = service.host(true);
    println!("host: {:?}; {}", host.resources, host.gpu.summary());
    let started = Instant::now();
    let status = service
        .start(
            StartOptions {
                model_id: model.id.clone(),
                backend: Backend::Auto,
                context_window: None,
                threads: None,
            },
            &cancel,
        )
        .unwrap();
    let load = started.elapsed();
    let plan = status.plan.clone().expect("start records its plan");
    println!(
        "start: ready in {load:?} (pid {:?}, {:?}); {} threads, context {}, estimated RAM {:?} MiB",
        status.pid,
        status.endpoint,
        plan.threads,
        plan.context_window,
        plan.estimated_ram_bytes.map(|bytes| bytes >> 20)
    );
    for reason in &plan.reasons {
        println!("  - {reason}");
    }
    assert_eq!(status.state, "ready");
    assert!(plan.reasons[0].contains("CPU"));

    let provider = registry
        .provider(&ProviderId::new(LOCAL_PROVIDER_ID).unwrap())
        .expect("local provider registered");
    let mut request = ChatRequest::new(
        ModelId::new(model.id.clone()).unwrap(),
        vec![ChatMessage::user(
            "Write a Rust function named add that returns the sum of two i32 values. Reply with code only.",
        )],
    )
    .with_max_tokens(96);
    request.temperature = Some(0.0);
    let started = Instant::now();
    let mut first_token = None;
    let response = provider
        .chat(
            &request,
            &mut |event| {
                if matches!(event, StreamEvent::TextDelta { .. }) {
                    first_token.get_or_insert_with(|| started.elapsed());
                }
            },
            &cancel,
        )
        .unwrap();
    let elapsed = started.elapsed();
    let completion = response.usage.completion_tokens.unwrap_or(0);
    let generating = first_token.map_or(elapsed, |first| elapsed.saturating_sub(first));
    println!(
        "chat: {} prompt + {completion} completion tokens in {elapsed:?}; first token after {:?}; ~{:.1} tok/s generation (client-measured)\n{}",
        response.usage.prompt_tokens.unwrap_or(0),
        first_token,
        completion as f64 / generating.as_secs_f64().max(f64::EPSILON),
        response.message.content
    );
    assert!(
        response.message.content.contains("fn"),
        "{}",
        response.message.content
    );

    let mut structured = ChatRequest::new(
        ModelId::new(model.id.clone()).unwrap(),
        vec![ChatMessage::user(
            "Return a JSON object for a person named Ada who is 36 years old.",
        )],
    )
    .with_response_format(ResponseFormat::JsonSchema {
        name: "person".into(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {"name": {"type": "string"}, "age": {"type": "integer"}},
            "required": ["name", "age"]
        }),
        strict: true,
    })
    .with_max_tokens(64);
    structured.temperature = Some(0.0);
    let response = provider.chat(&structured, &mut |_| {}, &cancel).unwrap();
    println!("json_schema: {}", response.message.content);
    let value: serde_json::Value = serde_json::from_str(&response.message.content).unwrap();
    assert!(
        value["name"].is_string() && value["age"].is_i64(),
        "{value}"
    );

    let stopped = service.stop().unwrap();
    assert_eq!(stopped.state, "stopped");
    assert!(registry.providers().is_empty());
}
