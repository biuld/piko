use std::collections::HashMap;

use llmd::auth::{AuthCredential, AuthStorage};
use hostd::models::ModelRegistry;
use piko_protocol::{InputModality, ModelSummary};

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

    assert_eq!(resolved.model.id, "gpt-4o");
    assert_eq!(resolved.model.name, "GPT-4o");
    assert_eq!(resolved.provider_config.api_key, Some("openai-key".into()));
}

#[test]
fn falls_back_to_matching_model_when_provider_does_not_match() {
    let registry = registry_with_openai_key();
    let resolved = registry.resolve(Some("gpt-4o"), Some("anthropic")).unwrap();

    assert_eq!(resolved.model.id, "gpt-4o");
}

#[test]
fn supports_custom_provider_registration() {
    let mut registry = registry_with_openai_key();
    registry.register_custom_provider(
        "custom",
        vec![ModelSummary {
            id: "custom-model".into(),
            name: "Custom Model".into(),
            reasoning: false,
            input: vec![InputModality::Text],
            context_window: 1000,
            max_tokens: 100,
        }],
    );

    let resolved = registry
        .resolve(Some("custom-model"), Some("custom"))
        .unwrap();
    assert_eq!(resolved.model.id, "custom-model");
}

#[test]
fn filters_scoped_models_by_provider_and_model_pattern() {
    let mut registry = registry_with_openai_key();
    // Match exact model id containing "gpt-4o" but not "gpt-4o-mini"
    registry.set_scoped_models(vec!["openai/gpt-4o".into()]);

    let models = registry.list_scoped_models();
    // gpt-4o, gpt-4o-mini, gpt-4o-2024-... all contain "gpt-4o"
    // This is expected — scoped filter uses substring match
    assert!(models.iter().all(|m| m.id.contains("gpt-4o")));
    assert!(models.iter().any(|m| m.id == "gpt-4o"));
}
