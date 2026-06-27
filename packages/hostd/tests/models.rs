use std::collections::HashMap;

use hostd::auth::{AuthCredential, AuthStorage};
use hostd::models::ModelRegistry;
use piko_protocol::ModelCatalogEntry;

fn registry_with_openai_key() -> ModelRegistry {
    let mut auth = HashMap::new();
    auth.insert(
        "openai".into(),
        AuthCredential::ApiKey {
            key: "openai-key".into(),
        },
    );
    ModelRegistry::new(AuthStorage::in_memory(auth), vec![])
}

#[test]
fn resolves_default_model_with_auth_config() {
    let registry = registry_with_openai_key();
    let resolved = registry.resolve(Some("gpt-4o"), Some("openai")).unwrap();

    assert_eq!(resolved.model.provider, "openai");
    assert_eq!(resolved.provider_config.api_key, Some("openai-key".into()));
}

#[test]
fn falls_back_to_matching_model_when_provider_does_not_match() {
    let registry = registry_with_openai_key();
    let resolved = registry.resolve(Some("gpt-4o"), Some("anthropic")).unwrap();

    assert_eq!(resolved.model.provider, "openai");
}

#[test]
fn supports_custom_provider_registration() {
    let mut registry = registry_with_openai_key();
    registry.register_custom_provider(
        "custom",
        vec![ModelCatalogEntry {
            id: "custom-model".into(),
            name: "Custom Model".into(),
            api: "custom-api".into(),
            provider: "custom".into(),
            base_url: Some("http://localhost:9999".into()),
            reasoning: false,
            input: vec!["text".into()],
            context_window: 1000,
            max_tokens: 100,
        }],
    );

    let resolved = registry
        .resolve(Some("custom-model"), Some("custom"))
        .unwrap();
    assert_eq!(resolved.model.provider, "custom");
    assert_eq!(
        resolved.provider_config.base_url,
        Some("http://localhost:9999".into())
    );
}

#[test]
fn filters_scoped_models_by_provider_and_model_pattern() {
    let mut registry = registry_with_openai_key();
    registry.set_scoped_models(vec!["openai/4o".into()]);

    let models = registry.list_scoped_models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "gpt-4o");
}
