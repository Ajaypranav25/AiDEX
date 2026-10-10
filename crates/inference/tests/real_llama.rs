//! Real llama.cpp validation (ignored by default). Run with the staged assets:
//!
//! ```text
//! AIDEX_TEST_LLAMA_ZIP=<llama-b11205-bin-win-cpu-x64.zip> \
//! AIDEX_TEST_GGUF=<qwen2.5-coder-0.5b-instruct-q4_k_m.gguf> \
//! cargo test -p aidex-inference --test real_llama -- --ignored --nocapture
//! ```
//!
//! It installs the pinned runtime and the model into a temporary drive,
//! composes a plan (with the opt-in GPU probe), loads the model through the
//! sandbox, completes prompts fully offline (loopback only, `--offline`),
//! benchmarks, and unloads, printing timings.

mod support;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use aidex_core::CancelToken;
use aidex_events::MemoryEventSink;
use aidex_inference::{
    ImportOptions, InferenceOptions, LoadOptions, LocalInference, gguf::read_metadata,
};
use aidex_providers::{ChatMessage, ChatRequest, ModelProvider, ResponseFormat, StreamEvent};
use serde_json::json;
use support::{port_is_closed, sandbox, temp_drive};

fn env_path(name: &str) -> PathBuf {
    let value = std::env::var_os(name)
        .unwrap_or_else(|| panic!("set {name} to run the real llama.cpp validation"));
    PathBuf::from(value)
}

fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

#[test]
#[ignore = "needs AIDEX_TEST_LLAMA_ZIP and AIDEX_TEST_GGUF (real llama.cpp runtime and model)"]
fn real_runtime_runs_the_staged_model_offline() {
    let archive = env_path("AIDEX_TEST_LLAMA_ZIP");
    let gguf = env_path("AIDEX_TEST_GGUF");
    let (_root, paths) = temp_drive();
    let events = Arc::new(MemoryEventSink::new());
    let options = InferenceOptions {
        idle_timeout: None,
        ..InferenceOptions::default()
    };
    let service = LocalInference::open(&paths, sandbox(), events.clone(), options).unwrap();
    let cancel = CancelToken::new();

    let started = Instant::now();
    let runtime = service
        .install_runtime(&archive, &mut |_| {}, &cancel)
        .unwrap();
    println!(
        "runtime: installed {} ({} files, tree {}) in {:?}",
        runtime.manifest.catalog_id,
        runtime.manifest.files.len(),
        runtime.manifest.tree_sha256,
        started.elapsed()
    );

    let metadata = read_metadata(&gguf).unwrap();
    let license_file = gguf.with_file_name("LICENSE");
    let started = Instant::now();
    let entry = service
        .import(
            &gguf,
            &ImportOptions {
                id: Some("qwen2.5-coder-0.5b".into()),
                license: Some("Apache-2.0".into()),
                license_file: license_file.is_file().then_some(license_file),
                source_url: Some(
                    "https://huggingface.co/Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF".into(),
                ),
                source_revision: Some("ebb2015119c907b064c512bf053e945850b5875f".into()),
                ..ImportOptions::default()
            },
            &mut |_| {},
            &cancel,
        )
        .unwrap();
    println!(
        "model: imported {} ({} MiB, {}, {:?}, ctx {}) in {:?}; sha256 {}",
        entry.id,
        mib(entry.size_bytes),
        entry.architecture,
        entry.quantization,
        entry.context_length,
        started.elapsed(),
        entry.sha256
    );
    assert_eq!(entry.architecture, metadata.architecture);
    let started = Instant::now();
    let verified = service
        .verify(&entry.id, true, &mut |_| {}, &cancel)
        .unwrap();
    println!("verify: full SHA-256 in {:?}", started.elapsed());
    assert_eq!(verified.sha256, entry.sha256);

    let load_options = LoadOptions {
        probe_gpu: true,
        ..LoadOptions::default()
    };
    let report = service.plan(&entry.id, &load_options).unwrap();
    println!(
        "host: {:?}; {}",
        report.host.resources,
        report.host.gpu.summary()
    );
    println!(
        "plan: {} threads {} ctx {} batch {} gpu_layers {} mmap {} est RAM {} MiB",
        report.plan.runtime.catalog_id,
        report.plan.threads,
        report.plan.ctx_size,
        report.plan.batch_size,
        report.plan.gpu_layers,
        report.plan.mmap,
        mib(report.plan.estimated_ram_bytes)
    );
    for reason in &report.plan.reasons {
        println!("  - {reason}");
    }

    let started = Instant::now();
    let load = service.load(&entry.id, &load_options, &cancel).unwrap();
    println!(
        "load: {:?} total, server ready after {} ms (pid {}, port {}, log {})",
        started.elapsed(),
        load.server.load_ms,
        load.server.pid,
        load.server.port,
        load.server.log
    );
    let provider = service.provider();
    let mut request = ChatRequest::new(
        entry.id.model_id().clone(),
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
    println!(
        "chat: {} prompt + {completion} completion tokens in {elapsed:?} (first token {:?}, ~{:.1} tok/s overall)\n{}",
        response.usage.prompt_tokens.unwrap_or(0),
        first_token,
        completion as f64 / elapsed.as_secs_f64(),
        response.message.content
    );
    assert!(
        response.message.content.contains("fn"),
        "{}",
        response.message.content
    );

    let schema = json!({
        "type": "object",
        "properties": {"name": {"type": "string"}, "age": {"type": "integer"}},
        "required": ["name", "age"]
    });
    let mut structured = ChatRequest::new(
        entry.id.model_id().clone(),
        vec![ChatMessage::user(
            "Return a JSON object for a person named Ada who is 36 years old.",
        )],
    )
    .with_response_format(ResponseFormat::JsonSchema {
        name: "person".into(),
        schema,
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

    let benchmark = service.benchmark(&entry.id, true, &cancel).unwrap();
    println!(
        "benchmark: load {} ms, prompt {} tok @ {:.1} tok/s, generate {} tok @ {:.1} tok/s, RAM {:?} MiB, recommended {} (host {})",
        benchmark.load_ms,
        benchmark.prompt_tokens,
        benchmark.prompt_tokens_per_second,
        benchmark.generated_tokens,
        benchmark.generation_tokens_per_second,
        benchmark.ram_bytes.map(mib),
        benchmark.recommended,
        benchmark.host_fingerprint
    );
    assert!(benchmark.generation_tokens_per_second > 0.0);

    let port = load.server.port;
    assert!(service.unload(&entry.id));
    assert!(port_is_closed(port));
    std::thread::sleep(Duration::from_millis(100));
    println!("events: {:?}", events.names());
    assert_eq!(events.names(), ["model.loaded", "model.unloaded"]);
}
