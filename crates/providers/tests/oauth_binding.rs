use aidex_providers::{AuthConfig, ProviderConfig, ProviderId, ProviderKind};

fn oauth(id: &str, kind: ProviderKind, url: &str) -> ProviderConfig {
    let mut config = ProviderConfig::new(ProviderId::new(id).unwrap(), kind, url);
    config.auth = AuthConfig::VaultSecret {
        secret_name: format!("oauth.{id}.access"),
    };
    config
}

#[test]
fn chatgpt_tokens_are_bound_to_the_registered_account_and_trusted_resource() {
    assert!(
        oauth(
            "openai",
            ProviderKind::OpenaiResponses,
            "https://api.openai.com/v1"
        )
        .validate()
        .is_ok()
    );
    assert!(
        oauth(
            "openai",
            ProviderKind::OpenaiResponses,
            "https://api.openai.com/v1/"
        )
        .validate()
        .is_ok()
    );
    for endpoint in [
        "https://example.com/v1",
        "http://api.openai.com/v1",
        "https://api.openai.com.evil.invalid/v1",
        "https://api.openai.com/v1/other",
        "https://api.openai.com/v1?other=1",
    ] {
        assert!(
            oauth("openai", ProviderKind::OpenaiResponses, endpoint)
                .validate()
                .is_err(),
            "OAuth token must not be usable at {endpoint}"
        );
    }
    assert!(
        oauth(
            "openai",
            ProviderKind::OpenaiCompatible,
            "https://api.openai.com/v1"
        )
        .validate()
        .is_err()
    );
    let mut different_account = oauth(
        "other",
        ProviderKind::OpenaiResponses,
        "https://api.openai.com/v1",
    );
    different_account.auth = AuthConfig::VaultSecret {
        secret_name: "oauth.openai.access".into(),
    };
    assert!(different_account.validate().is_err());
}

#[test]
fn standard_api_keys_still_support_user_configured_compatible_endpoints() {
    let mut config = ProviderConfig::new(
        ProviderId::new("custom").unwrap(),
        ProviderKind::OpenaiCompatible,
        "https://example.com/v1",
    );
    config.auth = AuthConfig::VaultSecret {
        secret_name: "provider-custom".into(),
    };
    assert!(config.validate().is_ok());
}
