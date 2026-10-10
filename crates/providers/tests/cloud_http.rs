mod support;
use aidex_providers::*;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use support::{FakeServer, Reply};

fn provider(kind: &str, server: &FakeServer) -> Arc<dyn ModelProvider> {
    let parsed = kind.parse::<ProviderKind>();
    assert!(
        parsed.is_ok(),
        "protocol {kind} must be configurable: {parsed:?}"
    );
    let mut config = ProviderConfig::new(
        ProviderId::new("account").unwrap(),
        parsed.unwrap(),
        server.base_url(),
    );
    config.discover_models = false;
    config.models.push(ModelConfig::new(
        ModelId::new("fixture-model").unwrap(),
        4096,
    ));
    config.auth = AuthConfig::VaultSecret {
        secret_name: "key".into(),
    };
    let keys = Arc::new(StaticCredentials::new());
    keys.insert(
        config.id.clone(),
        "key",
        SecretBytes::from("fixture-secret".to_owned()),
    );
    let registry = ProviderRegistry::in_memory(keys, Arc::new(OfflineMode::new(false)));
    registry.add_provider(config).unwrap();
    registry
        .provider(&ProviderId::new("account").unwrap())
        .unwrap()
}
fn request() -> ChatRequest {
    ChatRequest::new(
        ModelId::new("fixture-model").unwrap(),
        vec![ChatMessage::user("hello")],
    )
}

#[test]
fn multiline_sse_event_is_bounded_before_a_separator_arrives() {
    for kind in [
        "openai-compatible",
        "openai-responses",
        "anthropic",
        "gemini",
        "azure-openai",
    ] {
        let line = format!("data: {}\n", "x".repeat(600_000));
        let server = FakeServer::start(vec![Reply::Sse(vec![
            (Duration::ZERO, line.repeat(3)),
            (Duration::from_secs(5), "\n".into()),
        ])]);
        let error = provider(kind, &server)
            .chat(
                &request(),
                &mut |_| {},
                &CancelToken::new().with_timeout(Duration::from_millis(250)),
            )
            .unwrap_err();
        assert_eq!(
            error,
            ProviderError::Protocol("SSE event exceeds the size limit".into()),
            "{kind}"
        );
    }
}

struct CountingResolver {
    resolutions: std::sync::atomic::AtomicUsize,
    fail: bool,
    cancel: Option<CancelToken>,
}
impl CredentialResolver for CountingResolver {
    fn resolve(&self, _: &ProviderId, _: &str) -> Result<SecretBytes, CredentialError> {
        self.resolutions
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(token) = &self.cancel {
            token.cancel();
        }
        if self.fail {
            Err(CredentialError::Locked)
        } else {
            Ok(SecretBytes::from("fixture-secret".to_owned()))
        }
    }
}
#[test]
fn prestopped_chat_resolves_no_credentials_and_opens_no_connections() {
    for kind in [
        "openai-compatible",
        "openai-responses",
        "anthropic",
        "gemini",
        "azure-openai",
    ] {
        for deadline in [false, true] {
            let server = FakeServer::start(Vec::new());
            let mut config = ProviderConfig::new(
                ProviderId::new("account").unwrap(),
                kind.parse().unwrap(),
                server.base_url(),
            );
            config.discover_models = false;
            config.models.push(ModelConfig::new(
                ModelId::new("fixture-model").unwrap(),
                4096,
            ));
            config.auth = AuthConfig::VaultSecret {
                secret_name: "key".into(),
            };
            let resolver = Arc::new(CountingResolver {
                resolutions: std::sync::atomic::AtomicUsize::new(0),
                fail: true,
                cancel: None,
            });
            let registry =
                ProviderRegistry::in_memory(resolver.clone(), Arc::new(OfflineMode::new(false)));
            registry.add_provider(config).unwrap();
            let token = CancelToken::new();
            let token = if deadline {
                token.with_timeout(Duration::ZERO)
            } else {
                token.cancel();
                token
            };
            let mut events = Vec::new();
            let result = registry
                .provider(&ProviderId::new("account").unwrap())
                .unwrap()
                .chat(&request(), &mut |e| events.push(e.clone()), &token);
            assert_eq!(
                result.unwrap_err(),
                if deadline {
                    ProviderError::Timeout
                } else {
                    ProviderError::Cancelled
                },
                "{kind}"
            );
            assert_eq!(
                resolver
                    .resolutions
                    .load(std::sync::atomic::Ordering::SeqCst),
                0,
                "{kind}"
            );
            assert_eq!(
                events,
                if deadline {
                    vec![]
                } else {
                    vec![StreamEvent::Done {
                        finish_reason: FinishReason::Cancelled,
                    }]
                }
            );
            std::thread::sleep(Duration::from_millis(20));
            assert_eq!(server.connections(), 0, "{kind}");
        }
    }
}

#[test]
fn cancellation_during_credential_preparation_prevents_transport_admission() {
    for kind in [
        "openai-compatible",
        "openai-responses",
        "anthropic",
        "gemini",
        "azure-openai",
    ] {
        let server = FakeServer::start(Vec::new());
        let mut config = ProviderConfig::new(
            ProviderId::new("account").unwrap(),
            kind.parse().unwrap(),
            server.base_url(),
        );
        config.discover_models = false;
        config.models.push(ModelConfig::new(
            ModelId::new("fixture-model").unwrap(),
            4096,
        ));
        config.auth = AuthConfig::VaultSecret {
            secret_name: "key".into(),
        };
        let token = CancelToken::new();
        let resolver = Arc::new(CountingResolver {
            resolutions: std::sync::atomic::AtomicUsize::new(0),
            fail: false,
            cancel: Some(token.clone()),
        });
        let registry =
            ProviderRegistry::in_memory(resolver.clone(), Arc::new(OfflineMode::new(false)));
        registry.add_provider(config).unwrap();
        let mut events = Vec::new();
        assert_eq!(
            registry
                .provider(&ProviderId::new("account").unwrap())
                .unwrap()
                .chat(&request(), &mut |e| events.push(e.clone()), &token)
                .unwrap_err(),
            ProviderError::Cancelled
        );
        assert_eq!(
            events,
            vec![StreamEvent::Done {
                finish_reason: FinishReason::Cancelled
            }]
        );
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(server.connections(), 0, "{kind}");
    }
}

#[test]
fn gemini_signed_parallel_calls_survive_persistence_and_tool_continuation() {
    let signed = json!({"functionCall":{"id":"signed-call","name":"lookup","args":{"x":1}},"thoughtSignature":"c2lnbmVkLW9wYXF1ZQ=="});
    let unsigned = json!({"functionCall":{"id":"parallel-call","name":"lookup","args":{"x":2}}});
    let first = json!({"candidates":[{"content":{"parts":[signed.clone(), unsigned.clone()]},"finishReason":"STOP"}]}).to_string();
    let server = FakeServer::start(vec![
        Reply::sse(&[&first]),
        Reply::sse(&[
            r#"{"candidates":[{"content":{"parts":[{"text":"complete"}]},"finishReason":"STOP"}]}"#,
        ]),
    ]);
    let p = provider("gemini", &server);
    let response = p
        .chat(&request(), &mut |_| {}, &CancelToken::new())
        .unwrap();
    // Same serde path used to save/restore the assistant turn.
    let saved: ChatMessage =
        serde_json::from_value(serde_json::to_value(response.message).unwrap()).unwrap();
    let continuation = ChatRequest::new(
        ModelId::new("fixture-model").unwrap(),
        vec![
            ChatMessage::user("hello"),
            saved,
            ChatMessage::tool_result("signed-call", "one"),
            ChatMessage::tool_result("parallel-call", "two"),
        ],
    );
    let response = p
        .chat(&continuation, &mut |_| {}, &CancelToken::new())
        .unwrap();
    assert_eq!(response.message.content, "complete");
    let body = server.requests()[1].json();
    assert_eq!(body["contents"][1]["parts"][0], signed);
    assert_eq!(body["contents"][1]["parts"][1], unsigned);
    assert_eq!(body["contents"].as_array().unwrap().len(), 3);
    assert_eq!(body["contents"][2]["parts"].as_array().unwrap().len(), 2);
    assert_eq!(
        body["contents"][2]["parts"][1]["functionResponse"]["id"],
        "parallel-call"
    );
}

#[test]
fn gemini_tool_declarations_preserve_neutral_json_schema() {
    let schema = json!({"type":"object","properties":{"x":{"$ref":"#/$defs/X"}},"$defs":{"X":{"type":"integer"}},"additionalProperties":false});
    let server = FakeServer::start(vec![Reply::sse(&[
        r#"{"candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"STOP"}]}"#,
    ])]);
    let r = request().with_tools(vec![ToolSpec {
        name: "lookup".into(),
        description: "look up".into(),
        parameters: schema.clone(),
    }]);
    provider("gemini", &server)
        .chat(&r, &mut |_| {}, &CancelToken::new())
        .unwrap();
    let body = server.requests()[0].json();
    assert_eq!(
        body["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"],
        schema
    );
    assert!(
        body["tools"][0]["functionDeclarations"][0]
            .get("parameters")
            .is_none()
    );
}

#[test]
fn gemini_blocked_finishes_are_content_filter_and_malformed_finishes_fail() {
    for stop in [
        "BLOCKLIST",
        "PROHIBITED_CONTENT",
        "SPII",
        "SAFETY",
        "RECITATION",
        "IMAGE_SAFETY",
        "IMAGE_PROHIBITED_CONTENT",
        "IMAGE_RECITATION",
        "ESCALATION",
        "PUP_LIMITED_DISABLED",
    ] {
        let event = json!({"candidates":[{"finishReason":stop}]}).to_string();
        let server = FakeServer::start(vec![Reply::sse(&[&event])]);
        let response = provider("gemini", &server)
            .chat(&request(), &mut |_| {}, &CancelToken::new())
            .unwrap();
        assert_eq!(
            response.finish_reason,
            FinishReason::ContentFilter,
            "{stop}"
        );
    }
    for stop in [
        "MALFORMED_FUNCTION_CALL",
        "MALFORMED_RESPONSE",
        "UNEXPECTED_TOOL_CALL",
        "MISSING_THOUGHT_SIGNATURE",
        "TOO_MANY_TOOL_CALLS",
        "OTHER",
        "LANGUAGE",
        "FINISH_REASON_UNSPECIFIED",
        "unknown-private-prompt",
    ] {
        let event = json!({"candidates":[{"finishReason":stop}]}).to_string();
        let server = FakeServer::start(vec![Reply::sse(&[&event])]);
        let mut events = Vec::new();
        let error = provider("gemini", &server)
            .chat(
                &request(),
                &mut |e| events.push(e.clone()),
                &CancelToken::new(),
            )
            .unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::Protocol);
        assert!(!error.to_string().contains(stop));
        assert!(!events.iter().any(|e| matches!(e, StreamEvent::Done { .. })));
    }
}

#[test]
fn gemini_refuses_modified_signed_call_before_network() {
    let event = json!({"candidates":[{"content":{"parts":[{"functionCall":{"id":"signed-call","name":"lookup","args":{"x":1}},"thoughtSignature":"c2lnbmF0dXJl"}]},"finishReason":"STOP"}]}).to_string();
    let server = FakeServer::start(vec![Reply::sse(&[&event])]);
    let p = provider("gemini", &server);
    let mut assistant = p
        .chat(&request(), &mut |_| {}, &CancelToken::new())
        .unwrap()
        .message;
    assistant.tool_calls[0].arguments = "{\"x\":2}".into();
    let continuation = ChatRequest::new(
        ModelId::new("fixture-model").unwrap(),
        vec![
            ChatMessage::user("hello"),
            assistant,
            ChatMessage::tool_result("signed-call", "result"),
        ],
    );
    assert_eq!(
        p.chat(&continuation, &mut |_| {}, &CancelToken::new())
            .unwrap_err()
            .kind(),
        ProviderErrorKind::InvalidRequest
    );
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn bare_compatible_client_precancellation_wins_over_malformed_credentials() {
    let server = FakeServer::start(Vec::new());
    let client = OpenAiClient::new(
        BaseUrl::parse(&server.base_url()).unwrap(),
        HttpOptions::default(),
    );
    let cancel = CancelToken::new();
    cancel.cancel();
    let mut events = Vec::new();
    assert_eq!(
        client
            .chat(
                &request(),
                Some(&SecretBytes::from("invalid credential".to_owned())),
                &mut |e| events.push(e.clone()),
                &cancel
            )
            .unwrap_err(),
        ProviderError::Cancelled
    );
    assert_eq!(
        events,
        vec![StreamEvent::Done {
            finish_reason: FinishReason::Cancelled
        }]
    );
    assert_eq!(server.connections(), 0);
}

#[test]
fn responses_assembles_text_split_arguments_and_usage() {
    let server = FakeServer::start(vec![Reply::sse(&[
        r#"{"type":"response.output_text.delta","delta":"Hi"}"#,
        r#"{"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","call_id":"call1","name":"lookup","arguments":""}}"#,
        r#"{"type":"response.function_call_arguments.delta","output_index":1,"delta":"{\"x\":"}"#,
        r#"{"type":"response.function_call_arguments.delta","output_index":1,"delta":"1}"}"#,
        r#"{"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":5,"output_tokens":7}}}"#,
    ])]);
    let mut events = Vec::new();
    let result = provider("openai-responses", &server)
        .chat(
            &request(),
            &mut |e| events.push(e.clone()),
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(result.message.content, "Hi");
    assert_eq!(result.message.tool_calls[0].arguments, "{\"x\":1}");
    assert_eq!(result.finish_reason, FinishReason::ToolCalls);
    assert_eq!(result.usage.prompt_tokens, Some(5));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, StreamEvent::Done { .. }))
            .count(),
        1
    );
    let req = &server.requests()[0];
    assert_eq!(req.path, "/v1/responses");
    assert_eq!(req.json()["input"][0]["role"], "user");
    assert_eq!(req.header("authorization"), Some("Bearer fixture-secret"));
}

#[test]
fn anthropic_assembles_blocks_and_uses_api_key_header() {
    let server = FakeServer::start(vec![Reply::sse(&[
        r#"{"type":"message_start","message":{"usage":{"input_tokens":5,"output_tokens":0}}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#,
        r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call1","name":"lookup","input":{}}}"#,
        r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"x\":"}}"#,
        r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"1}"}}"#,
        r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":7}}"#,
        r#"{"type":"message_stop"}"#,
    ])]);
    let result = provider("anthropic", &server)
        .chat(&request(), &mut |_| {}, &CancelToken::new())
        .unwrap();
    assert_eq!(result.message.content, "Hi");
    assert_eq!(result.message.tool_calls[0].arguments, "{\"x\":1}");
    assert_eq!(
        result.usage,
        Usage {
            prompt_tokens: Some(5),
            completion_tokens: Some(7)
        }
    );
    let req = &server.requests()[0];
    assert_eq!(req.path, "/v1/messages");
    assert_eq!(req.header("x-api-key"), Some("fixture-secret"));
    assert_eq!(req.header("anthropic-version"), Some("2023-06-01"));
    assert!(req.header("authorization").is_none());
}

#[test]
fn gemini_assembles_functions_and_uses_header_key() {
    let server = FakeServer::start(vec![Reply::sse(&[
        r#"{"candidates":[{"content":{"parts":[{"text":"Hi"}]}}]}"#,
        r#"{"candidates":[{"content":{"parts":[{"functionCall":{"id":"call1","name":"lookup","args":{"x":1}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":7}}"#,
    ])]);
    let result = provider("gemini", &server)
        .chat(&request(), &mut |_| {}, &CancelToken::new())
        .unwrap();
    assert_eq!(result.message.content, "Hi");
    assert_eq!(result.message.tool_calls[0].arguments, "{\"x\":1}");
    assert_eq!(result.finish_reason, FinishReason::ToolCalls);
    let req = &server.requests()[0];
    assert_eq!(
        req.path,
        "/v1/models/fixture-model:streamGenerateContent?alt=sse"
    );
    assert_eq!(req.header("x-goog-api-key"), Some("fixture-secret"));
    assert_eq!(req.json()["contents"][0]["parts"][0]["text"], "hello");
}

#[test]
fn azure_uses_deployment_url_and_api_key() {
    let server = FakeServer::start(vec![Reply::sse(&[
        r#"{"choices":[{"delta":{"content":"Hi"},"finish_reason":"stop"}]}"#,
        "[DONE]",
    ])]);
    let result = provider("azure-openai", &server)
        .chat(&request(), &mut |_| {}, &CancelToken::new())
        .unwrap();
    assert_eq!(result.message.content, "Hi");
    let req = &server.requests()[0];
    assert_eq!(
        req.path,
        "/v1/deployments/fixture-model/chat/completions?api-version=2024-10-21"
    );
    assert_eq!(req.header("api-key"), Some("fixture-secret"));
    assert!(req.header("authorization").is_none());
}

#[test]
fn cloud_errors_are_typed_and_do_not_echo_secrets_or_server_text() {
    for protocol in ["openai-responses", "anthropic", "gemini", "azure-openai"] {
        for (status, kind) in [
            (401, ProviderErrorKind::Unauthorized),
            (429, ProviderErrorKind::RateLimited),
            (503, ProviderErrorKind::Unavailable),
        ] {
            let server = FakeServer::start(vec![Reply::Json(
                status,
                json!({"error":{"message":"fixture-secret private-prompt"}}).to_string(),
            )]);
            let err = provider(protocol, &server)
                .chat(&request(), &mut |_| {}, &CancelToken::new())
                .unwrap_err();
            assert_eq!(err.kind(), kind);
            assert!(!format!("{err:?} {err}").contains("fixture-secret"));
            assert!(!err.to_string().contains("private-prompt"));
        }
    }
}

#[test]
fn cloud_rejects_malformed_and_unterminated_streams() {
    for protocol in ["openai-responses", "anthropic", "gemini", "azure-openai"] {
        for event in ["{not-json fixture-secret", "{}"] {
            let server = FakeServer::start(vec![Reply::sse(&[event])]);
            let err = provider(protocol, &server)
                .chat(&request(), &mut |_| {}, &CancelToken::new())
                .unwrap_err();
            assert_eq!(err.kind(), ProviderErrorKind::Protocol);
            assert!(!err.to_string().contains("fixture-secret"));
        }
    }
}

#[test]
fn cloud_cancellation_emits_one_cancelled_done_promptly() {
    for protocol in ["openai-responses", "anthropic", "gemini", "azure-openai"] {
        let server = FakeServer::start(vec![Reply::Sse(vec![(
            Duration::from_secs(5),
            "data: {}\n\n".into(),
        )])]);
        let p = provider(protocol, &server);
        let cancel = CancelToken::new();
        let other = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            other.cancel();
        });
        let started = std::time::Instant::now();
        let mut events = Vec::new();
        assert_eq!(
            p.chat(&request(), &mut |e| events.push(e.clone()), &cancel)
                .unwrap_err(),
            ProviderError::Cancelled
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(
            events,
            vec![StreamEvent::Done {
                finish_reason: FinishReason::Cancelled
            }]
        );
    }
}

#[test]
fn missing_cloud_credentials_report_auth_required_without_network() {
    let server = FakeServer::start(Vec::new());
    let mut config = ProviderConfig::new(
        ProviderId::new("locked").unwrap(),
        "anthropic".parse().unwrap(),
        server.base_url(),
    );
    config.auth = AuthConfig::VaultSecret {
        secret_name: "missing".into(),
    };
    let registry = ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        Arc::new(OfflineMode::new(false)),
    );
    registry.add_provider(config).unwrap();
    let health = registry
        .health(&ProviderId::new("locked").unwrap())
        .unwrap();
    assert_eq!(
        serde_json::to_value(health).unwrap()["status"],
        "auth_required"
    );
    assert_eq!(server.connections(), 0);
}

#[test]
fn server_machine_codes_cannot_echo_a_credential() {
    for kind in [
        "openai-compatible",
        "openai-responses",
        "anthropic",
        "gemini",
        "azure-openai",
    ] {
        let server = FakeServer::start(vec![Reply::Json(
            400,
            json!({"error":{"code":"fixture-secret","type":"fixture-secret"}}).to_string(),
        )]);
        let error = provider(kind, &server)
            .chat(&request(), &mut |_| {}, &CancelToken::new())
            .unwrap_err();
        assert!(
            !format!("{error:?} {error}").contains("fixture-secret"),
            "{error:?}"
        );
    }
}

#[test]
fn malformed_compatible_stream_errors_cannot_echo_a_credential() {
    let server = FakeServer::start(vec![Reply::sse(&[r#"{"choices":"fixture-secret"}"#])]);
    let error = provider("openai-compatible", &server)
        .chat(&request(), &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Protocol);
    assert!(
        !format!("{error:?} {error}").contains("fixture-secret"),
        "{error:?}"
    );
}

#[test]
fn compatible_constructor_refuses_noncompatible_protocols() {
    let config = ProviderConfig::new(
        ProviderId::new("account").unwrap(),
        ProviderKind::Anthropic,
        "http://127.0.0.1:1/v1",
    );
    assert!(
        OpenAiCompatibleProvider::from_config(
            &config,
            Arc::new(NoVault::default()),
            Arc::new(OfflineMode::new(false))
        )
        .is_err()
    );
}

#[test]
fn responses_discovery_retains_context_metadata_and_custom_overrides() {
    let server = FakeServer::start(vec![Reply::Json(200, json!({"data":[{"id":"discovered","context_window":32768},{"id":"custom","context_window":2048}]}).to_string())]);
    let mut config = ProviderConfig::new(
        ProviderId::new("account").unwrap(),
        ProviderKind::OpenaiResponses,
        server.base_url(),
    );
    config
        .models
        .push(ModelConfig::new(ModelId::new("custom").unwrap(), 65536));
    let registry = ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        Arc::new(OfflineMode::new(false)),
    );
    registry.add_provider(config).unwrap();
    let list = registry.models();
    assert!(list.failures.is_empty());
    assert_eq!(
        list.models
            .iter()
            .find(|m| m.model.model_id.as_str() == "custom")
            .unwrap()
            .context_window,
        65536
    );
    let found = list
        .models
        .iter()
        .find(|m| m.model.model_id.as_str() == "discovered")
        .unwrap();
    assert_eq!(found.context_window, 32768);
    assert_eq!(found.source, CapabilitySource::Discovered);
}
