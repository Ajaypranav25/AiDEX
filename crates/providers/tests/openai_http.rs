//! OpenAI-compatible provider against an in-test HTTP server: SSE streaming,
//! tool calls, JSON-schema passthrough, discovery, errors, and cancellation.

mod support;

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use aidex_providers::{
    AuthConfig, CancelToken, CapabilitySource, ChatMessage, ChatRequest, FinishReason,
    HealthStatus, Locality, ModelConfig, ModelId, ModelProvider, NoVault, OfflineMode,
    OpenAiCompatibleProvider, ProviderConfig, ProviderError, ProviderId, ProviderKind,
    ResponseFormat, StreamEvent, ToolCallSupport, ToolSpec,
};
use serde_json::json;
use support::{FakeServer, Reply};

fn provider_for(
    server: &FakeServer,
    configure: impl FnOnce(&mut ProviderConfig),
) -> OpenAiCompatibleProvider {
    let mut config = ProviderConfig::new(
        ProviderId::new("local-server").unwrap(),
        ProviderKind::OpenaiCompatible,
        server.base_url(),
    );
    configure(&mut config);
    OpenAiCompatibleProvider::from_config(
        &config,
        Arc::new(NoVault::default()),
        Arc::new(OfflineMode::new(false)),
    )
    .unwrap()
}

fn request(text: &str) -> ChatRequest {
    ChatRequest::new(ModelId::new("qwen").unwrap(), vec![ChatMessage::user(text)])
}

#[test]
fn streams_text_deltas_and_usage_from_server_sent_events() {
    let server = FakeServer::start(vec![Reply::sse(&[
        r#"{"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":""}}]}"#,
        r#"{"choices":[{"index":0,"delta":{"content":"Hello"}}]}"#,
        r#"{"choices":[{"index":0,"delta":{"content":", world"}}]}"#,
        r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
        r#"{"choices":[],"usage":{"prompt_tokens":9,"completion_tokens":3,"total_tokens":12}}"#,
        "[DONE]",
    ])]);
    let provider = provider_for(&server, |_| {});
    assert_eq!(provider.descriptor().locality, Locality::Local);
    let mut events = Vec::new();
    let response = provider
        .chat(
            &request("hi"),
            &mut |event| events.push(event.clone()),
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(response.message.content, "Hello, world");
    assert_eq!(response.finish_reason, FinishReason::Stop);
    assert_eq!(response.usage.prompt_tokens, Some(9));
    assert_eq!(response.usage.completion_tokens, Some(3));
    let deltas: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, ["Hello", ", world"]);
    assert_eq!(
        events.last(),
        Some(&StreamEvent::Done {
            finish_reason: FinishReason::Stop
        })
    );

    let sent = &server.requests()[0];
    assert_eq!(
        (sent.method.as_str(), sent.path.as_str()),
        ("POST", "/v1/chat/completions")
    );
    assert!(sent.header("authorization").is_none());
    let body = sent.json();
    assert_eq!(body["model"], "qwen");
    assert_eq!(body["stream"], true);
    assert_eq!(
        body["messages"][0],
        json!({"role": "user", "content": "hi"})
    );
}

#[test]
fn streamed_tool_calls_are_assembled_and_json_schema_is_passed_through() {
    let server = FakeServer::start(vec![Reply::Sse(vec![
        (Duration::ZERO, "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_9\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"\"}}]}}]}\n\n".into()),
        // Split across TCP writes mid-event to prove line reassembly.
        (Duration::from_millis(20), "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":".into()),
        (Duration::from_millis(20), "\\\"src/main.rs\\\"}\"}}]}}]}\r\n\r\n".into()),
        (Duration::ZERO, "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n".into()),
    ])]);
    let provider = provider_for(&server, |_| {});
    let schema = json!({"type": "object", "properties": {"action": {"type": "string"}}, "required": ["action"]});
    let request = request("read main")
        .with_tools(vec![ToolSpec {
            name: "read_file".into(),
            description: "Read a workspace file".into(),
            parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        }])
        .with_response_format(ResponseFormat::JsonSchema {
            name: "action".into(),
            schema: schema.clone(),
            strict: true,
        });
    let mut deltas = 0;
    let response = provider
        .chat(
            &request,
            &mut |event| {
                if matches!(event, StreamEvent::ToolCallDelta { .. }) {
                    deltas += 1;
                }
            },
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(response.finish_reason, FinishReason::ToolCalls);
    // The arguments arrived in two TCP writes but form one SSE event.
    assert_eq!(deltas, 2);
    let call = &response.message.tool_calls[0];
    assert_eq!(
        (call.id.as_str(), call.name.as_str()),
        ("call_9", "read_file")
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&call.arguments).unwrap(),
        json!({"path": "src/main.rs"})
    );
    let body = server.requests()[0].json();
    assert_eq!(body["tools"][0]["function"]["name"], "read_file");
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(body["response_format"]["json_schema"]["schema"], schema);
}

#[test]
fn a_json_answer_to_a_streaming_request_is_still_understood() {
    let server = FakeServer::start(vec![Reply::Json(
        200,
        r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"plain"},"finish_reason":"length"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#.into(),
    )]);
    let mut events = Vec::new();
    let response = provider_for(&server, |_| {})
        .chat(
            &request("x"),
            &mut |event| events.push(event.clone()),
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(response.message.content, "plain");
    assert_eq!(response.finish_reason, FinishReason::Length);
    assert!(matches!(
        events.first(),
        Some(StreamEvent::TextDelta { .. })
    ));
}

#[test]
fn http_failures_become_typed_errors() {
    let server = FakeServer::start(vec![
        Reply::Json(401, r#"{"error":{"message":"bad key","code":"invalid_api_key"}}"#.into()),
        Reply::Raw(429, vec![("Retry-After".into(), "3".into())], "{}".into()),
        Reply::Json(
            400,
            r#"{"error":{"code":400,"message":"the request exceeds the available context size","type":"exceed_context_size_error","n_prompt_tokens":9000,"n_ctx":4096}}"#.into(),
        ),
        Reply::Json(502, "{}".into()),
        Reply::Raw(302, vec![("Location".into(), "http://elsewhere.invalid/".into())], String::new()),
        Reply::sse(&[r#"{"choices":[{"index":0,"delta":{"content":"half"}}]}"#]),
    ]);
    let provider = provider_for(&server, |_| {});
    let call = || provider.chat(&request("x"), &mut |_| {}, &CancelToken::new());
    assert_eq!(call().unwrap_err(), ProviderError::Unauthorized);
    assert_eq!(
        call().unwrap_err(),
        ProviderError::RateLimited {
            retry_after: Some(Duration::from_secs(3))
        }
    );
    assert_eq!(
        call().unwrap_err(),
        ProviderError::ContextOverflow {
            limit: Some(4096),
            requested: Some(9000)
        }
    );
    let bad_gateway = call().unwrap_err();
    assert!(bad_gateway.is_retryable(), "{bad_gateway:?}");
    assert_eq!(
        call().unwrap_err(),
        ProviderError::Http {
            status: 302,
            code: None
        },
        "redirects are never followed"
    );
    assert!(
        matches!(call().unwrap_err(), ProviderError::Protocol(_)),
        "truncated stream"
    );
    assert_eq!(server.connections(), 6);
}

#[test]
fn unreachable_servers_are_unavailable_not_success() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let config = ProviderConfig::new(
        ProviderId::new("gone").unwrap(),
        ProviderKind::OpenaiCompatible,
        format!("http://{addr}/v1"),
    );
    let provider = OpenAiCompatibleProvider::from_config(
        &config,
        Arc::new(NoVault::default()),
        Arc::new(OfflineMode::new(false)),
    )
    .unwrap();
    let error = provider
        .chat(&request("x"), &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert!(
        matches!(error, ProviderError::Unavailable { .. }),
        "{error:?}"
    );
    assert!(error.is_retryable());
    let health = provider.health();
    assert_eq!(health.status, HealthStatus::Unavailable);
    assert!(provider.models().is_err());
}

#[test]
fn cancellation_returns_promptly_while_the_server_is_still_streaming() {
    let server = FakeServer::start(vec![Reply::Sse(vec![
        (
            Duration::ZERO,
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"first\"}}]}\n\n".into(),
        ),
        (
            Duration::from_secs(20),
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"late\"}}]}\n\n".into(),
        ),
    ])]);
    let provider = provider_for(&server, |_| {});
    let cancel = CancelToken::new();
    let trigger = cancel.clone();
    let mut events = Vec::new();
    let started = Instant::now();
    let error = provider
        .chat(
            &request("x"),
            &mut |event| {
                if matches!(event, StreamEvent::TextDelta { .. }) {
                    let trigger = trigger.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(50));
                        trigger.cancel();
                    });
                }
                events.push(event.clone());
            },
            &cancel,
        )
        .unwrap_err();
    assert_eq!(error, ProviderError::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        events.last(),
        Some(&StreamEvent::Done {
            finish_reason: FinishReason::Cancelled
        })
    );

    let deadline = CancelToken::new().with_timeout(Duration::from_millis(100));
    server.push(Reply::Sse(vec![(
        Duration::from_secs(20),
        "data: [DONE]\n\n".into(),
    )]));
    let started = Instant::now();
    assert_eq!(
        provider
            .chat(&request("x"), &mut |_| {}, &deadline)
            .unwrap_err(),
        ProviderError::Timeout
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn discovery_merges_declared_models_and_marks_unknown_limits_assumed() {
    let server = FakeServer::start(vec![
        Reply::Json(
            200,
            r#"{"object":"list","data":[{"id":"qwen","object":"model","meta":{"n_ctx_train":32768}},{"id":"declared"},{"id":"mystery"}]}"#.into(),
        ),
        Reply::Json(200, r#"{"object":"list","data":[]}"#.into()),
    ]);
    let provider = provider_for(&server, |config| {
        let mut declared = ModelConfig::new(ModelId::new("declared").unwrap(), 128_000);
        declared.tool_calls = ToolCallSupport::Native;
        config.models.push(declared);
    });
    let models = provider.models().unwrap();
    let summary: Vec<_> = models
        .iter()
        .map(|model| {
            (
                model.model.model_id.as_str(),
                model.context_window,
                model.source,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("declared", 128_000, CapabilitySource::Configured),
            ("qwen", 32_768, CapabilitySource::Discovered),
            ("mystery", 4096, CapabilitySource::Assumed),
        ]
    );
    assert_eq!(models[0].capabilities.tool_calls, ToolCallSupport::Native);
    // A second listing within the cache TTL does not hit the server.
    provider.models().unwrap();
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.requests()[0].path, "/v1/models");
    assert_eq!(provider.health().status, HealthStatus::Ready);

    let declared_only = provider_for(&server, |config| {
        config.discover_models = false;
        config
            .models
            .push(ModelConfig::new(ModelId::new("only").unwrap(), 2048));
    });
    assert_eq!(declared_only.models().unwrap().len(), 1);
    assert_eq!(
        server.requests().len(),
        2,
        "declared-only listing needs no network"
    );
}

#[test]
fn locality_and_transport_rules_are_enforced_at_configuration() {
    let build = |url: &str, locality: Option<Locality>, auth: AuthConfig| {
        let mut config = ProviderConfig::new(
            ProviderId::new("p").unwrap(),
            ProviderKind::OpenaiCompatible,
            url,
        );
        config.locality = locality;
        config.auth = auth;
        OpenAiCompatibleProvider::from_config(
            &config,
            Arc::new(NoVault::default()),
            Arc::new(OfflineMode::new(false)),
        )
    };
    let secret = || AuthConfig::VaultSecret {
        secret_name: "key".into(),
    };
    assert!(
        build(
            "https://api.example.com/v1",
            Some(Locality::Local),
            AuthConfig::None
        )
        .is_err()
    );
    assert!(build("http://api.example.com/v1", None, secret()).is_err());
    assert!(build("https://user:pw@api.example.com/v1", None, AuthConfig::None).is_err());
    let lan = build("http://192.168.1.20:8000/v1", None, AuthConfig::None).unwrap();
    assert_eq!(lan.descriptor().locality, Locality::Remote);
    let tunnel = build("http://127.0.0.1:9/v1", Some(Locality::Remote), secret()).unwrap();
    assert_eq!(tunnel.descriptor().locality, Locality::Remote);
    let cloud = build("https://api.example.com", None, secret()).unwrap();
    assert_eq!(
        cloud.descriptor().base_url.as_deref(),
        Some("https://api.example.com/v1")
    );
}
