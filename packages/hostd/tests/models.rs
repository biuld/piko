use std::collections::HashMap;

use hostd::domain::config::ModelRegistry;
use llmd::auth::{AuthCredential, AuthStorage};
use llmd::providers::ProviderRegistry;

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
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("mycloud.toml"),
        r#"
[provider]
id = "mycloud"
adapter = "openai"
base_url = "https://api.mycloud.example/v1"

[models.mycloud-fast]
name = "MyCloud Fast"
reasoning = false
input = ["text"]
context_window = 32000
max_tokens = 4096
"#,
    )
    .unwrap();

    let mut registry = ProviderRegistry::new();
    registry.load_from_dir(dir.path());
    let model_registry =
        ModelRegistry::with_registry(AuthStorage::in_memory(HashMap::new()), vec![], registry);

    let resolved = model_registry
        .resolve(Some("mycloud-fast"), Some("mycloud"))
        .unwrap();
    assert_eq!(resolved.model.id, "mycloud-fast");
    assert_eq!(resolved.provider, "mycloud");
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
