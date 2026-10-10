//! Credentials are resolved per request and appear only in the Authorization
//! header: never in configuration files, `Debug` output, errors, events, or
//! the request body sent to the model.

mod support;

use std::sync::Arc;

use aidex_providers::{
    AuthConfig, CancelToken, ChatMessage, ChatRequest, CredentialError, CredentialResolver,
    ModelId, ModelObserver, ModelRef, NoVault, OfflineMode, ProviderConfig, ProviderError,
    ProviderId, ProviderKind, ProviderRegistry, RecordingObserver, SecretBytes, StaticCredentials,
};
use support::{FakeServer, Reply, files_under, sentinel_secret, temp_drive};

struct LockedVault;

impl CredentialResolver for LockedVault {
    fn resolve(&self, _: &ProviderId, _: &str) -> Result<SecretBytes, CredentialError> {
        Err(CredentialError::Locked)
    }
}

fn config(server: &FakeServer, id: &ProviderId) -> ProviderConfig {
    let mut config = ProviderConfig::new(
        id.clone(),
        ProviderKind::OpenaiCompatible,
        server.base_url(),
    );
    config.auth = AuthConfig::VaultSecret {
        secret_name: "api-key".into(),
    };
    config
}

#[test]
fn the_credential_reaches_only_the_authorization_header() {
    let secret = sentinel_secret();
    let id = ProviderId::new("cloud").unwrap();
    let server = FakeServer::start(vec![
        Reply::Json(
            200,
            r#"{"object":"list","data":[{"id":"gpt","context_length":8192}]}"#.into(),
        ),
        Reply::sse(&[
            r#"{"choices":[{"index":0,"delta":{"content":"ok"},"finish_reason":"stop"}]}"#,
            "[DONE]",
        ]),
        // The server echoes the key back in its error text.
        Reply::Json(
            401,
            format!(
                r#"{{"error":{{"message":"Incorrect API key provided: {secret}","code":"invalid_api_key"}}}}"#
            ),
        ),
    ]);
    let (root, paths) = temp_drive();
    let credentials = Arc::new(StaticCredentials::new());
    credentials.insert(id.clone(), "api-key", SecretBytes::from(secret.clone()));
    let observer = Arc::new(RecordingObserver::new());
    let registry = ProviderRegistry::load(
        &paths,
        credentials.clone(),
        Arc::new(OfflineMode::new(false)),
    )
    .unwrap()
    .with_observer(observer.clone());
    registry.add_provider(config(&server, &id)).unwrap();
    let model = ModelRef::new(id.clone(), ModelId::new("gpt").unwrap());
    registry.set_default_model(model.clone()).unwrap();

    let provider = registry.provider(&id).unwrap();
    let request = ChatRequest::new(model.model_id.clone(), vec![ChatMessage::user("hello")]);
    let mut streamed = String::new();
    provider
        .chat(
            &request,
            &mut |event| streamed.push_str(&format!("{event:?}")),
            &CancelToken::new(),
        )
        .unwrap();
    let error = provider
        .chat(&request, &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert_eq!(error, ProviderError::Unauthorized);

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for sent in &requests {
        assert_eq!(
            sent.header("authorization"),
            Some(format!("Bearer {secret}").as_str())
        );
        assert!(
            !String::from_utf8_lossy(&sent.body).contains(&secret),
            "credential leaked into the request body (model context)"
        );
    }

    let mut surfaces = vec![
        format!("{error} {error:?}"),
        format!("{registry:?}"),
        format!("{provider:?}", provider = provider.descriptor()),
        format!("{credentials:?}"),
        streamed,
        serde_json::to_string(&observer.events()).unwrap(),
        serde_json::to_string(&registry.providers()).unwrap(),
        serde_json::to_string(&registry.models()).unwrap(),
        serde_json::to_string(&provider.health()).unwrap(),
    ];
    for file in files_under(root.path()) {
        surfaces.push(String::from_utf8_lossy(&std::fs::read(file).unwrap()).into_owned());
    }
    for surface in &surfaces {
        assert!(!surface.contains(&secret), "credential leaked: {surface}");
    }
    let saved = std::fs::read_to_string(paths.config().join("providers.toml")).unwrap();
    assert!(saved.contains("secret_name = \"api-key\""), "{saved}");
    assert!(!observer.events().is_empty());
}

#[test]
fn credential_failures_send_nothing() {
    let server = FakeServer::start(vec![]);
    let id = ProviderId::new("cloud").unwrap();
    let request = ChatRequest::new(ModelId::new("gpt").unwrap(), vec![ChatMessage::user("x")]);
    for (resolver, expected) in [
        (
            Arc::new(LockedVault) as Arc<dyn CredentialResolver>,
            CredentialError::Locked,
        ),
        (
            Arc::new(NoVault::default()),
            CredentialError::Unavailable {
                reason: "vault unavailable".into(),
            },
        ),
        (
            Arc::new(StaticCredentials::new()),
            CredentialError::NotFound {
                secret_name: "api-key".into(),
            },
        ),
    ] {
        let registry = ProviderRegistry::in_memory(resolver, Arc::new(OfflineMode::new(false)));
        registry.add_provider(config(&server, &id)).unwrap();
        let provider = registry.provider(&id).unwrap();
        let error = provider
            .chat(&request, &mut |_| {}, &CancelToken::new())
            .unwrap_err();
        assert_eq!(error, ProviderError::Credential(expected));
        assert!(!error.is_retryable());
    }
    let malformed = Arc::new(StaticCredentials::new());
    malformed.insert(
        id.clone(),
        "api-key",
        SecretBytes::from("two words".to_owned()),
    );
    let registry = ProviderRegistry::in_memory(malformed, Arc::new(OfflineMode::new(false)));
    registry.add_provider(config(&server, &id)).unwrap();
    let error = registry
        .provider(&id)
        .unwrap()
        .chat(&request, &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert!(matches!(
        error,
        ProviderError::Credential(CredentialError::Malformed { .. })
    ));
    assert!(!error.to_string().contains("two words"));
    assert_eq!(server.connections(), 0);
}

#[test]
fn observer_events_name_providers_not_secrets() {
    let observer = RecordingObserver::new();
    observer.observe(&aidex_providers::ModelEvent::NetworkModeChanged { offline: true });
    let json = serde_json::to_value(observer.events()).unwrap();
    assert_eq!(json[0]["kind"], "network.mode_changed");
}
