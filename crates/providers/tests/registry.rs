//! Registry persistence, runtime providers, aggregated listings, and defaults.

mod support;

use std::sync::Arc;

use aidex_providers::{
    ConfigError, FakeProvider, Locality, ModelDescriptor, ModelEvent, ModelId, ModelRef, NoVault,
    OfflineMode, ProviderConfig, ProviderError, ProviderId, ProviderKind, ProviderOrigin,
    ProviderRegistry, RecordingObserver, RegistryError,
};
use support::{FakeServer, Reply, temp_drive};

fn fake(id: &str, models: &[&str]) -> FakeProvider {
    let provider_id = ProviderId::new(id).unwrap();
    models.iter().fold(
        FakeProvider::new(provider_id.clone(), Locality::Local),
        |fake, model| {
            fake.with_model(ModelDescriptor::new(
                ModelRef::new(provider_id.clone(), ModelId::new(*model).unwrap()),
                Locality::Local,
                8192,
            ))
        },
    )
}

fn load(paths: &aidex_drive::DrivePaths) -> ProviderRegistry {
    ProviderRegistry::load(
        paths,
        Arc::new(NoVault::default()),
        Arc::new(OfflineMode::new(false)),
    )
    .unwrap()
}

#[test]
fn configured_providers_and_the_default_model_persist_across_loads() {
    let (_root, paths) = temp_drive();
    let server = FakeServer::start(vec![Reply::Json(
        200,
        r#"{"object":"list","data":[{"id":"qwen","meta":{"n_ctx_train":32768}}]}"#.into(),
    )]);
    let observer = Arc::new(RecordingObserver::new());
    let registry = load(&paths).with_observer(observer.clone());
    assert!(registry.providers().is_empty());
    let id = ProviderId::new("llama").unwrap();
    let descriptor = registry
        .add_provider(ProviderConfig::new(
            id.clone(),
            ProviderKind::OpenaiCompatible,
            server.base_url(),
        ))
        .unwrap();
    assert_eq!(descriptor.locality, Locality::Local);
    let model = ModelRef::parse("llama/qwen").unwrap();
    let chosen = registry.set_default_model(model.clone()).unwrap();
    assert_eq!(chosen.context_window, 32_768);
    assert!(matches!(
        registry.set_default_model(ModelRef::parse("llama/missing").unwrap()),
        Err(RegistryError::ModelNotListed(_))
    ));
    assert!(matches!(
        registry.add_provider(ProviderConfig::new(
            id.clone(),
            ProviderKind::OpenaiCompatible,
            server.base_url()
        )),
        Err(RegistryError::Duplicate(_))
    ));

    let reloaded = load(&paths);
    assert_eq!(reloaded.default_model(), Some(model));
    assert_eq!(reloaded.providers()[0].origin, ProviderOrigin::Configured);

    reloaded.remove_provider(&id).unwrap();
    assert!(reloaded.providers().is_empty());
    assert_eq!(
        reloaded.default_model(),
        None,
        "default cleared with its provider"
    );
    let reloaded = load(&paths);
    assert!(reloaded.providers().is_empty());

    let names: Vec<_> = observer.events().iter().map(ModelEvent::name).collect();
    assert_eq!(names, ["provider.added", "model.default_changed"]);
}

#[test]
fn runtime_providers_are_listed_but_never_persisted() {
    let (_root, paths) = temp_drive();
    let registry = load(&paths);
    registry
        .register(Box::new(fake("local", &["tiny", "coder"])))
        .unwrap();
    assert!(matches!(
        registry.register(Box::new(fake("local", &[]))),
        Err(RegistryError::Duplicate(_))
    ));
    let entries = registry.providers();
    assert_eq!(entries[0].origin, ProviderOrigin::Runtime);
    let listing = registry.models();
    let names: Vec<_> = listing.models.iter().map(|m| m.model.to_string()).collect();
    assert_eq!(names, ["local/coder", "local/tiny"]);
    assert!(matches!(
        registry.remove_provider(&ProviderId::new("local").unwrap()),
        Err(RegistryError::NotConfigured(_))
    ));
    registry
        .set_default_model(ModelRef::parse("local/tiny").unwrap())
        .unwrap();
    let reloaded = load(&paths);
    assert!(reloaded.providers().is_empty());
    assert_eq!(
        reloaded.default_model(),
        Some(ModelRef::parse("local/tiny").unwrap())
    );
    registry
        .unregister(&ProviderId::new("local").unwrap())
        .unwrap();
    assert!(registry.providers().is_empty());
}

#[test]
fn listing_failures_are_reported_next_to_working_providers() {
    let registry = ProviderRegistry::in_memory(
        Arc::new(NoVault::default()),
        Arc::new(OfflineMode::new(false)),
    );
    registry.register(Box::new(fake("good", &["m"]))).unwrap();
    let broken = fake("broken", &["m"]);
    broken.set_models_error(Some(ProviderError::unavailable("server down")));
    registry.register(Box::new(broken)).unwrap();
    let listing = registry.models();
    assert_eq!(listing.models.len(), 1);
    assert_eq!(listing.failures.len(), 1);
    assert_eq!(listing.failures[0].provider_id.as_str(), "broken");
    assert!(listing.failures[0].message.contains("server down"));
    assert!(matches!(
        registry.set_default_model(ModelRef::parse("broken/m").unwrap()),
        Err(RegistryError::Provider { .. })
    ));
}

#[test]
fn invalid_configuration_files_are_rejected_explicitly() {
    let (_root, paths) = temp_drive();
    let path = paths.config().join("providers.toml");
    std::fs::create_dir_all(paths.config()).unwrap();
    std::fs::write(&path, "version = 2\n").unwrap();
    let error = ProviderRegistry::load(
        &paths,
        Arc::new(NoVault::default()),
        Arc::new(OfflineMode::new(false)),
    )
    .unwrap_err();
    assert!(
        matches!(
            error,
            RegistryError::Config(ConfigError::UnsupportedVersion { found: 2, .. })
        ),
        "{error:?}"
    );
    std::fs::write(
        &path,
        "version = 1\n[[provider]]\nid = \"x\"\nkind = \"openai-compatible\"\nbase_url = \"https://api.example.com/v1\"\napi_key = \"sk-pasted\"\n",
    )
    .unwrap();
    let error = ProviderRegistry::load(
        &paths,
        Arc::new(NoVault::default()),
        Arc::new(OfflineMode::new(false)),
    )
    .unwrap_err();
    assert!(
        matches!(error, RegistryError::Config(ConfigError::Invalid { .. })),
        "{error:?}"
    );
    assert!(!error.to_string().contains("sk-pasted"));
    std::fs::write(
        &path,
        "version = 1\n[[provider]]\nid = \"x\"\nkind = \"openai-compatible\"\nbase_url = \"https://api.example.com/v1\"\nlocality = \"local\"\n",
    )
    .unwrap();
    assert!(matches!(
        ProviderRegistry::load(
            &paths,
            Arc::new(NoVault::default()),
            Arc::new(OfflineMode::new(false))
        ),
        Err(RegistryError::Invalid(_))
    ));
}
