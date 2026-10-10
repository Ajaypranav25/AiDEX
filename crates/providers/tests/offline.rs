//! Offline mode refuses remote provider traffic before any socket is opened,
//! while loopback (local) providers keep working.

mod support;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use aidex_providers::{
    AuthConfig, CancelToken, ChatMessage, ChatRequest, CredentialError, CredentialResolver,
    HealthStatus, Locality, ModelId, ModelProvider, OfflineMode, OpenAiCompatibleProvider,
    ProviderConfig, ProviderError, ProviderId, ProviderKind, ProviderRegistry, SecretBytes,
};
use support::{FakeServer, Reply};

/// Counts how often a credential was requested.
#[derive(Default)]
struct CountingResolver(AtomicUsize);

impl CredentialResolver for CountingResolver {
    fn resolve(&self, _: &ProviderId, _: &str) -> Result<SecretBytes, CredentialError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(SecretBytes::from("sk-offline-test".to_owned()))
    }
}

fn request() -> ChatRequest {
    ChatRequest::new(ModelId::new("gpt").unwrap(), vec![ChatMessage::user("x")])
}

#[test]
fn offline_mode_refuses_remote_calls_with_zero_connections() {
    let server = FakeServer::start(vec![]);
    let mut config = ProviderConfig::new(
        ProviderId::new("cloud").unwrap(),
        ProviderKind::OpenaiCompatible,
        server.base_url(),
    );
    // A loopback listener stands in for a cloud endpoint.
    config.locality = Some(Locality::Remote);
    config.auth = AuthConfig::VaultSecret {
        secret_name: "cloud-key".into(),
    };
    let resolver = Arc::new(CountingResolver::default());
    let offline = Arc::new(OfflineMode::new(true));
    let provider =
        OpenAiCompatibleProvider::from_config(&config, resolver.clone(), offline.clone()).unwrap();

    let chat = provider
        .chat(&request(), &mut |_| {}, &CancelToken::new())
        .unwrap_err();
    assert!(
        matches!(chat, ProviderError::OfflineRefused { .. }),
        "{chat:?}"
    );
    assert!(!chat.is_retryable());
    assert!(matches!(
        provider.models().unwrap_err(),
        ProviderError::OfflineRefused { .. }
    ));
    let health = provider.health();
    assert_eq!(health.status, HealthStatus::Refused);

    assert_eq!(server.connections(), 0, "offline mode opened a connection");
    assert_eq!(
        resolver.0.load(Ordering::SeqCst),
        0,
        "credentials were resolved while offline"
    );

    // Going online lets the same provider through.
    offline.set_offline(false).unwrap();
    server.push(Reply::Json(
        200,
        r#"{"object":"list","data":[{"id":"gpt"}]}"#.into(),
    ));
    assert_eq!(provider.models().unwrap().len(), 1);
    assert_eq!(server.connections(), 1);
    assert_eq!(resolver.0.load(Ordering::SeqCst), 1);
}

#[test]
fn offline_mode_keeps_loopback_providers_available() {
    let server = FakeServer::start(vec![Reply::sse(&[
        r#"{"choices":[{"index":0,"delta":{"content":"local"},"finish_reason":"stop"}]}"#,
        "[DONE]",
    ])]);
    let config = ProviderConfig::new(
        ProviderId::new("llama").unwrap(),
        ProviderKind::OpenaiCompatible,
        server.base_url(),
    );
    let provider = OpenAiCompatibleProvider::from_config(
        &config,
        Arc::new(CountingResolver::default()),
        Arc::new(OfflineMode::new(true)),
    )
    .unwrap();
    assert_eq!(provider.descriptor().locality, Locality::Local);
    let response = provider
        .chat(&request(), &mut |_| {}, &CancelToken::new())
        .unwrap();
    assert_eq!(response.message.content, "local");
}

#[test]
fn registry_listing_reports_offline_refusals_instead_of_hiding_them() {
    let (_root, paths) = support::temp_drive();
    let server = FakeServer::start(vec![]);
    let offline = Arc::new(OfflineMode::load(&paths).unwrap());
    offline.set_offline(true).unwrap();
    let registry = ProviderRegistry::load(
        &paths,
        Arc::new(CountingResolver::default()),
        offline.clone(),
    )
    .unwrap();
    let mut config = ProviderConfig::new(
        ProviderId::new("cloud").unwrap(),
        ProviderKind::OpenaiCompatible,
        server.base_url(),
    );
    config.locality = Some(Locality::Remote);
    registry.add_provider(config).unwrap();
    let listing = registry.models();
    assert!(listing.models.is_empty());
    assert_eq!(listing.failures.len(), 1);
    assert_eq!(listing.failures[0].kind.as_str(), "offline_refused");
    assert_eq!(server.connections(), 0);
    // The switch survives a restart.
    assert!(OfflineMode::load(&paths).unwrap().is_offline());
}
