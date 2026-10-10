//! The managed llama-server path end to end against the fake runtime: install,
//! import, verify, compose, launch through the sandbox, chat (streaming,
//! cancellation, JSON-schema passthrough), scheduling, idle unload, failure
//! reporting, and the `ModelProvider` integration.

mod support;

use std::{
    fs,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use aidex_core::CancelToken;
use aidex_inference::{
    ImportOptions, InferenceError, LoadOptions, LocalInference, LocalModelId, SchedulerPolicy,
    runtime::install_runtime,
};
use aidex_providers::{
    ChatMessage, ChatRequest, FinishReason, Locality, ModelProvider, NoVault, OfflineMode,
    ProviderError, ProviderRegistry, ResponseFormat, StreamEvent, ToolCallSupport,
};
use serde_json::json;
use support::{
    contains_hex_key, fake_model, fake_runtime_archive, fake_state, files_under, http, options,
    port_is_closed, service, temp_drive, wait_until, write_model,
};

struct Fixture {
    _root: tempfile::TempDir,
    _sources: tempfile::TempDir,
    sources: std::path::PathBuf,
    paths: aidex_drive::DrivePaths,
    service: LocalInference,
    events: Arc<aidex_events::MemoryEventSink>,
}

fn fixture(options: aidex_inference::InferenceOptions) -> Fixture {
    let (root, paths) = temp_drive();
    let sources = tempfile::tempdir().unwrap();
    let (archive, entry) = fake_runtime_archive(sources.path());
    install_runtime(&paths, &archive, &entry).unwrap();
    let (service, events) = service(&paths, &entry, options);
    Fixture {
        sources: sources.path().to_path_buf(),
        _root: root,
        _sources: sources,
        paths,
        service,
        events,
    }
}

impl Fixture {
    fn import(&self, id: &str, builder: aidex_inference::gguf::GgufBuilder) -> LocalModelId {
        let file = write_model(&self.sources, id, builder);
        self.service
            .import(
                &file,
                &ImportOptions {
                    id: Some(id.into()),
                    ..ImportOptions::default()
                },
                &mut |_| {},
                &CancelToken::new(),
            )
            .unwrap()
            .id
    }

    fn event_names(&self) -> Vec<String> {
        self.events
            .events()
            .into_iter()
            .map(|envelope| {
                let data = serde_json::to_value(&envelope.event).unwrap();
                format!(
                    "{}:{}",
                    envelope.event.name(),
                    data["data"]["model_id"].as_str().unwrap_or_default()
                )
            })
            .collect()
    }
}

fn request(id: &LocalModelId, prompt: &str) -> ChatRequest {
    ChatRequest::new(id.model_id().clone(), vec![ChatMessage::user(prompt)])
}

#[test]
fn a_registered_model_serves_streaming_chat_through_a_managed_server() {
    let fixture = fixture(options());
    let id = fixture.import("coder", fake_model("Coder"));
    let report = fixture
        .service
        .load(&id, &LoadOptions::default(), &CancelToken::new())
        .unwrap();
    assert!(!report.already_loaded);
    assert!(report.verification.is_some(), "verified before first use");
    assert_eq!(report.plan.runtime.variant.as_str(), "cpu");
    assert!(
        report
            .plan
            .reasons
            .iter()
            .any(|reason| reason.contains("using the CPU runtime")),
        "{:?}",
        report.plan.reasons
    );
    let port = report.server.port;

    let provider = fixture.service.provider();
    let mut deltas = Vec::new();
    let response = provider
        .chat(
            &request(&id, "hello"),
            &mut |event| {
                if let StreamEvent::TextDelta { text } = event {
                    deltas.push(text.clone());
                }
            },
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(response.message.content, "tok0 tok1 tok2 tok3 ");
    assert_eq!(deltas.len(), 4, "streamed token by token");
    assert_eq!(response.finish_reason, FinishReason::Stop);
    assert_eq!(response.usage.completion_tokens, Some(4));

    // The server was launched loopback-only, offline, with the key in its
    // environment rather than on its command line.
    let state = fake_state(port);
    let args: Vec<String> = serde_json::from_value(state["args"].clone()).unwrap();
    let flag = |name: &str| args.iter().position(|arg| arg == name);
    assert_eq!(args[flag("--host").unwrap() + 1], "127.0.0.1");
    for expected in ["--offline", "--jinja", "--no-webui", "--parallel"] {
        assert!(flag(expected).is_some(), "{expected} missing from {args:?}");
    }
    assert!(flag("--api-key").is_none());
    assert!(!args.iter().any(|arg| contains_hex_key(arg)), "{args:?}");
    let env_names: Vec<String> = serde_json::from_value(state["env_names"].clone()).unwrap();
    assert!(env_names.iter().any(|name| name == "LLAMA_API_KEY"));
    assert!(
        !env_names.iter().any(|name| name.starts_with("LLAMA_ARG_")),
        "no inherited llama.cpp settings: {env_names:?}"
    );

    // Requests without the per-launch key are refused; health is public.
    assert_eq!(http(port, "GET", "/v1/models", None).0, 401);
    assert_eq!(http(port, "GET", "/v1/models", Some("Bearer wrong")).0, 401);
    assert_eq!(http(port, "GET", "/health", None).0, 200);

    // Nothing that leaves the service carries the key.
    let loaded = serde_json::to_string(&fixture.service.loaded()).unwrap();
    assert!(!contains_hex_key(&loaded), "{loaded}");
    assert!(!contains_hex_key(&format!("{:?}", fixture.service)));
    let log = fs::read_to_string(
        fixture.service.logs_dir().join(
            std::path::Path::new(&report.server.log)
                .file_name()
                .unwrap(),
        ),
    )
    .unwrap();
    assert!(log.contains("ready after"), "{log}");
    assert!(!contains_hex_key(&log), "{log}");

    assert!(fixture.service.unload(&id));
    assert!(port_is_closed(port), "the server tree is gone after unload");
    assert!(!fixture.service.unload(&id));
    assert_eq!(
        fixture.event_names(),
        ["model.loaded:coder", "model.unloaded:coder"]
    );
    assert!(
        files_under(&fixture.paths.tmp()).is_empty(),
        "scratch removed: {:?}",
        files_under(&fixture.paths.tmp())
    );
}

#[test]
fn chat_loads_lazily_and_cancellation_aborts_the_request() {
    let fixture = fixture(options());
    let id = fixture.import(
        "slow",
        fake_model("Slow")
            .u64("aidex.fake.token_delay_ms", 200)
            .u64("aidex.fake.tokens", 200),
    );
    let provider = Arc::new(fixture.service.provider());
    let cancel = CancelToken::new();
    let first_delta = Arc::new(Mutex::new(None::<Instant>));
    let worker = {
        let provider = Arc::clone(&provider);
        let cancel = cancel.clone();
        let first_delta = Arc::clone(&first_delta);
        let request = request(&id, "write a lot");
        thread::spawn(move || {
            let mut events = Vec::new();
            let result = provider.chat(
                &request,
                &mut |event| {
                    if matches!(event, StreamEvent::TextDelta { .. }) {
                        first_delta.lock().unwrap().get_or_insert_with(Instant::now);
                    }
                    events.push(event.clone());
                },
                &cancel,
            );
            (result, events)
        })
    };
    wait_until("the first token", Duration::from_secs(30), || {
        first_delta.lock().unwrap().is_some()
    });
    let cancelled_at = Instant::now();
    cancel.cancel();
    let (result, events) = worker.join().unwrap();
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(2),
        "returned promptly"
    );
    assert_eq!(result.unwrap_err(), ProviderError::Cancelled);
    assert_eq!(
        events.last(),
        Some(&StreamEvent::Done {
            finish_reason: FinishReason::Cancelled
        })
    );
    // The server noticed the closed connection and stopped generating.
    let port = fixture.service.loaded()[0].server.port;
    wait_until(
        "the server to abandon the stream",
        Duration::from_secs(10),
        || fake_state(port)["abandoned"] == 1,
    );
    assert_eq!(fake_state(port)["completed"], 0);
    // The model stays resident for the next request.
    assert_eq!(fixture.service.loaded().len(), 1);
}

#[test]
fn json_schema_requests_reach_the_server_and_return_json() {
    let fixture = fixture(options());
    let id = fixture.import("structured", fake_model("Structured"));
    let schema = json!({
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "required": ["answer"]
    });
    let response = fixture
        .service
        .provider()
        .chat(
            &request(&id, "answer in JSON").with_response_format(ResponseFormat::JsonSchema {
                name: "answer".into(),
                schema: schema.clone(),
                strict: true,
            }),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&response.message.content).unwrap();
    assert_eq!(parsed["answer"], "fake");
    let port = fixture.service.loaded()[0].server.port;
    let recorded = &fake_state(port)["requests"][0];
    assert_eq!(recorded["response_format"]["type"], "json_schema");
    assert_eq!(recorded["response_format"]["json_schema"]["schema"], schema);
    assert_eq!(recorded["stream"], true);
}

#[test]
fn the_provider_lists_registered_models_for_routing() {
    let fixture = fixture(options());
    fixture.import("qwen-coder", fake_model("Qwen Coder"));
    let registry = ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        Arc::new(OfflineMode::new(true)),
    );
    registry
        .register(Box::new(fixture.service.provider()))
        .unwrap();
    let listing = registry.models();
    assert!(listing.failures.is_empty(), "{:?}", listing.failures);
    let model = &listing.models[0];
    assert_eq!(model.model.to_string(), "local/qwen-coder");
    assert_eq!(model.locality, Locality::Local);
    assert_eq!(model.capabilities.tool_calls, ToolCallSupport::Structured);
    assert!(model.capabilities.json_schema_output);
    assert_eq!(model.context_window, 4096, "the training context bounds it");
    assert!(model.min_ram_bytes.is_some());
    assert!(model.has_role(aidex_providers::ModelRole::Code));
    // Offline mode does not affect the local provider.
    let health = registry
        .health(&aidex_providers::ProviderId::new("local").unwrap())
        .unwrap();
    assert_eq!(
        health.status,
        aidex_providers::HealthStatus::Ready,
        "{health:?}"
    );
    let response = registry
        .provider(&model.model.provider_id)
        .unwrap()
        .chat(
            &ChatRequest::new(model.model.model_id.clone(), vec![ChatMessage::user("hi")]),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    assert!(!response.message.content.is_empty());
}

#[test]
fn idle_models_are_unloaded_after_the_timeout() {
    let mut options = options();
    options.idle_timeout = Some(Duration::from_millis(300));
    let fixture = fixture(options);
    let id = fixture.import("idle", fake_model("Idle"));
    let report = fixture
        .service
        .load(&id, &LoadOptions::default(), &CancelToken::new())
        .unwrap();
    // The reaper takes the model out of the resident set, then stops its
    // server and records the event.
    wait_until("the idle unload", Duration::from_secs(10), || {
        fixture.service.loaded().is_empty() && fixture.event_names().len() == 2
    });
    assert!(port_is_closed(report.server.port));
    assert_eq!(
        fixture.event_names(),
        ["model.loaded:idle", "model.unloaded:idle"]
    );
    // The next request loads it again.
    fixture
        .service
        .provider()
        .chat(&request(&id, "again"), &mut |_| {}, &CancelToken::new())
        .unwrap();
}

#[test]
fn only_one_large_model_is_resident_at_a_time() {
    let mut options = options();
    options.scheduler = SchedulerPolicy {
        large_model_bytes: 0,
        max_small_models: 2,
    };
    let fixture = fixture(options);
    let first = fixture.import("first", fake_model("First"));
    let second = fixture.import("second", fake_model("Second"));
    let one = fixture
        .service
        .load(&first, &LoadOptions::default(), &CancelToken::new())
        .unwrap();
    let two = fixture
        .service
        .load(&second, &LoadOptions::default(), &CancelToken::new())
        .unwrap();
    let resident: Vec<String> = fixture
        .service
        .loaded()
        .into_iter()
        .map(|model| model.model.to_string())
        .collect();
    assert_eq!(resident, ["second"]);
    assert!(port_is_closed(one.server.port));
    assert!(!port_is_closed(two.server.port));
    assert_eq!(
        fixture.event_names(),
        [
            "model.loaded:first",
            "model.unloaded:first",
            "model.loaded:second"
        ]
    );
    let again = fixture
        .service
        .load(&second, &LoadOptions::default(), &CancelToken::new())
        .unwrap();
    assert!(again.already_loaded);
}

#[test]
fn small_models_share_slots_and_busy_models_are_not_evicted() {
    let mut options = options();
    options.scheduler = SchedulerPolicy {
        large_model_bytes: u64::MAX,
        max_small_models: 1,
    };
    let fixture = fixture(options);
    let busy = fixture.import(
        "busy",
        fake_model("Busy")
            .u64("aidex.fake.token_delay_ms", 100)
            .u64("aidex.fake.tokens", 20),
    );
    let other = fixture.import("other", fake_model("Other"));
    let provider = Arc::new(fixture.service.provider());
    let worker = {
        let provider = Arc::clone(&provider);
        let request = request(&busy, "long answer");
        thread::spawn(move || provider.chat(&request, &mut |_| {}, &CancelToken::new()))
    };
    wait_until("the busy model to serve", Duration::from_secs(30), || {
        fixture
            .service
            .loaded()
            .first()
            .is_some_and(|model| model.in_flight == 1)
    });
    // With one slot and the resident model busy, a short deadline gives up.
    let error = fixture
        .service
        .load(
            &other,
            &LoadOptions::default(),
            &CancelToken::new().with_timeout(Duration::from_millis(300)),
        )
        .unwrap_err();
    assert_eq!(error.kind(), "busy", "{error}");
    // Without a deadline it waits for the request to finish, then evicts.
    fixture
        .service
        .load(&other, &LoadOptions::default(), &CancelToken::new())
        .unwrap();
    assert_eq!(
        worker.join().unwrap().unwrap().usage.completion_tokens,
        Some(20)
    );
    let resident: Vec<String> = fixture
        .service
        .loaded()
        .into_iter()
        .map(|model| model.model.to_string())
        .collect();
    assert_eq!(resident, ["other"]);
}

#[test]
fn a_server_that_exits_is_reported_with_its_log() {
    let fixture = fixture(options());
    let id = fixture.import(
        "broken",
        fake_model("Broken").u64("aidex.fake.exit_code", 7),
    );
    let error = fixture
        .service
        .load(&id, &LoadOptions::default(), &CancelToken::new())
        .unwrap_err();
    let InferenceError::ServerExited { code, detail, log } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(*code, Some(7));
    assert!(detail.contains("simulated failure"), "{detail}");
    let log = log.as_ref().unwrap();
    assert!(
        fs::read_to_string(log)
            .unwrap()
            .contains("simulated failure")
    );
    assert!(error.advice().unwrap().contains("server log"));
    assert!(fixture.service.loaded().is_empty());
    assert_eq!(fixture.event_names(), ["model.error:broken"]);
    // Through the provider it is an explicit unavailability, never a fake answer.
    let error = fixture
        .service
        .provider()
        .chat(&request(&id, "hi"), &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert!(
        matches!(error, ProviderError::Unavailable { .. }),
        "{error:?}"
    );
}

#[test]
fn slow_model_loads_wait_for_health_and_loads_can_be_cancelled() {
    let fixture = fixture(options());
    let id = fixture.import(
        "slow-start",
        fake_model("Slow").u64("aidex.fake.startup_ms", 800),
    );
    let report = fixture
        .service
        .load(&id, &LoadOptions::default(), &CancelToken::new())
        .unwrap();
    assert!(report.server.load_ms >= 800, "{}", report.server.load_ms);
    fixture.service.unload(&id);

    let hung = fixture.import(
        "hung",
        fake_model("Hung").u64("aidex.fake.startup_ms", 600_000),
    );
    let cancel = CancelToken::new();
    let canceller = {
        let cancel = cancel.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            cancel.cancel();
        })
    };
    let started = Instant::now();
    let error = fixture
        .service
        .load(&hung, &LoadOptions::default(), &cancel)
        .unwrap_err();
    canceller.join().unwrap();
    assert_eq!(error, InferenceError::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(fixture.service.loaded().is_empty());
}

#[test]
fn tampered_models_and_runtimes_are_refused_before_launch() {
    let fixture = fixture(options());
    let id = fixture.import("victim", fake_model("Victim"));
    let entry = fixture.service.registry().get(&id).unwrap();
    let model_path = entry.model_path(&fixture.paths).unwrap();
    let original = fs::read(&model_path).unwrap();
    let mut tampered = original.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x55;
    fs::write(&model_path, &tampered).unwrap();
    fs::File::options()
        .write(true)
        .open(&model_path)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(10))
        .unwrap();
    let error = fixture
        .service
        .load(&id, &LoadOptions::default(), &CancelToken::new())
        .unwrap_err();
    assert_eq!(error.kind(), "hash_mismatch", "{error}");
    fs::write(&model_path, &original).unwrap();

    let runtime = fixture.service.runtimes().unwrap().installed.runtimes[0].clone();
    let planted = runtime.dir.join("version.dll");
    fs::write(&planted, b"planted").unwrap();
    let error = fixture
        .service
        .load(&id, &LoadOptions::default(), &CancelToken::new())
        .unwrap_err();
    assert_eq!(error.kind(), "runtime_corrupt", "{error}");
    assert!(error.advice().unwrap().contains("aidex runtime install"));
    fs::remove_file(&planted).unwrap();
    fixture
        .service
        .load(&id, &LoadOptions::default(), &CancelToken::new())
        .unwrap();
}

#[test]
fn missing_runtimes_and_models_fail_with_advice() {
    let (_root, paths) = temp_drive();
    let sources = tempfile::tempdir().unwrap();
    let (_archive, entry) = fake_runtime_archive(sources.path());
    let (service, _events) = service(&paths, &entry, options());
    let file = write_model(sources.path(), "lonely", fake_model("Lonely"));
    let id = service
        .import(
            &file,
            &ImportOptions::default(),
            &mut |_| {},
            &CancelToken::new(),
        )
        .unwrap()
        .id;
    let error = service
        .load(&id, &LoadOptions::default(), &CancelToken::new())
        .unwrap_err();
    assert_eq!(error.kind(), "runtime_not_installed");
    assert!(
        error.to_string().contains("fake-llama-runtime.zip"),
        "{error}"
    );
    assert!(error.advice().unwrap().contains("aidex runtime install"));
    let health = service.provider().health();
    assert_eq!(health.status, aidex_providers::HealthStatus::Unavailable);
    assert!(health.message.unwrap().contains("aidex runtime install"));

    let missing = LocalModelId::new("missing").unwrap();
    assert_eq!(
        service
            .load(&missing, &LoadOptions::default(), &CancelToken::new())
            .unwrap_err()
            .kind(),
        "model_not_found"
    );
    let error = service
        .provider()
        .chat(&request(&missing, "hi"), &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert!(
        matches!(error, ProviderError::ModelNotFound { .. }),
        "{error:?}"
    );
}

#[test]
fn remove_deletes_only_what_aidex_imported() {
    let fixture = fixture(options());
    let id = fixture.import("removable", fake_model("Removable"));
    fixture
        .service
        .load(&id, &LoadOptions::default(), &CancelToken::new())
        .unwrap();
    let dir = fixture.paths.models().join("removable");
    fs::write(dir.join("notes.txt"), "user notes").unwrap();
    let removed = fixture.service.remove(&id).unwrap();
    assert!(
        fixture.service.loaded().is_empty(),
        "unloaded before removal"
    );
    assert!(
        removed
            .deleted
            .contains(&"models/removable/model.gguf".to_owned())
    );
    assert!(
        removed
            .deleted
            .contains(&"models/registry/removable.toml".to_owned())
    );
    assert_eq!(removed.kept, ["models/removable/notes.txt"]);
    assert_eq!(
        fs::read_to_string(dir.join("notes.txt")).unwrap(),
        "user notes"
    );
    assert_eq!(
        fixture.service.remove(&id).unwrap_err().kind(),
        "model_not_found"
    );
}

#[test]
fn benchmarks_report_speed_and_are_cached_per_host() {
    let fixture = fixture(options());
    let id = fixture.import("bench", fake_model("Bench"));
    let report = fixture
        .service
        .benchmark(&id, false, &CancelToken::new())
        .unwrap();
    assert!(!report.cached);
    assert_eq!(report.generated_tokens, 4);
    assert!(report.generation_tokens_per_second > 0.0);
    assert!(report.recommended);
    assert_eq!(report.host_fingerprint.len(), 16);
    let cache = fixture
        .paths
        .cache()
        .join("hosts")
        .join(&report.host_fingerprint)
        .join("benchmarks.json");
    assert!(cache.is_file());
    let cached = fixture
        .service
        .benchmark(&id, false, &CancelToken::new())
        .unwrap();
    assert!(cached.cached);
    assert_eq!(cached.measured_at, report.measured_at);
    let fresh = fixture
        .service
        .benchmark(&id, true, &CancelToken::new())
        .unwrap();
    assert!(!fresh.cached);
}
