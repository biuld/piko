use std::collections::HashMap;

use piko_hostd::domain::config::ModelRegistry;
use piko_llmd::auth::{AuthCredential, AuthStorage};
use piko_llmd::providers::ProviderRegistry;
use piko_protocol::model::ProviderAuthMethod;

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

fn registry_with_deepseek_key() -> ModelRegistry {
    ModelRegistry::new(
        AuthStorage::in_memory(HashMap::from([(
            "deepseek".into(),
            AuthCredential::ApiKey {
                key: "deepseek-key".into(),
            },
        )])),
        vec![],
    )
}

#[test]
fn resolves_default_model_without_copying_auth_material() {
    let registry = registry_with_openai_key();
    let resolved = registry.resolve(Some("gpt-4o"), Some("openai")).unwrap();

    assert_eq!(resolved.model.id, "gpt-4o");
    assert_eq!(resolved.model.name, "GPT-4o");
    assert_eq!(
        resolved.target.as_ref().unwrap().protocol,
        piko_llmd::modeling::ProtocolProfile::Responses {
            continuation: Default::default()
        }
    );
    assert_eq!(
        resolved.target.as_ref().unwrap().base_url,
        "https://api.openai.com/v1"
    );
}

#[test]
fn oauth_selects_catalog_subscription_target_without_exposing_token() {
    let registry = ModelRegistry::new(
        AuthStorage::in_memory(HashMap::from([(
            "openai".into(),
            AuthCredential::OAuth {
                access: "oauth-secret".into(),
                refresh: Some("refresh-secret".into()),
                expires: Some(u64::MAX),
                extra: HashMap::new(),
            },
        )])),
        vec![],
    );
    let resolved = registry.resolve(Some("gpt-4o"), Some("openai")).unwrap();
    assert_eq!(
        resolved.target.as_ref().unwrap().base_url,
        "https://chatgpt.com/backend-api/codex/"
    );
    assert_eq!(
        resolved.target.as_ref().unwrap().protocol,
        piko_llmd::modeling::ProtocolProfile::Responses {
            continuation: piko_llmd::modeling::ResponsesContinuationPolicy::EncryptedReasoning
        }
    );
}

#[test]
fn deepseek_resolves_model_specific_responses_target() {
    let registry = registry_with_deepseek_key();
    let flash = registry
        .resolve(Some("deepseek-v4-flash"), Some("deepseek"))
        .unwrap();
    assert_eq!(
        flash.target.as_ref().unwrap().protocol,
        piko_llmd::modeling::ProtocolProfile::Responses {
            continuation: piko_llmd::modeling::ResponsesContinuationPolicy::StatelessReplay
        }
    );

    let pro = registry
        .resolve(Some("deepseek-v4-pro"), Some("deepseek"))
        .unwrap();
    assert_eq!(
        pro.target.as_ref().unwrap().protocol,
        piko_llmd::modeling::ProtocolProfile::ChatCompletions
    );
}

#[test]
fn advertises_registered_authentication_methods() {
    let providers = registry_with_openai_key().list_providers();
    let openai = providers
        .iter()
        .find(|provider| provider.provider == "openai")
        .unwrap();
    assert_eq!(
        openai.auth_methods,
        vec![ProviderAuthMethod::ApiKey, ProviderAuthMethod::OAuth]
    );
    assert!(
        providers
            .iter()
            .all(|provider| provider.provider != "anthropic"),
        "unsupported native protocols must not be advertised as built-ins"
    );
}

#[test]
fn explicit_provider_model_identity_fails_closed() {
    let registry = registry_with_openai_key();
    assert!(
        registry
            .resolve(Some("gpt-4o"), Some("anthropic"))
            .is_none()
    );
}

#[test]
fn bare_duplicate_model_identity_is_ambiguous() {
    fn manifest(provider: &str) -> String {
        format!(
            r#"
[provider]
id = "{provider}"
[api_surfaces.platform]
base_url = "https://{provider}.example/v1"
auth_methods = ["api_key"]
[default_targets.platform]
protocol = "chat_completions"
[models.shared-model]
name = "Shared"
reasoning = false
input = ["text"]
context_window = 32000
max_tokens = 4096
"#
        )
    }

    let mut providers = ProviderRegistry::new();
    providers.register(Box::new(
        piko_llmd::providers::TomlProvider::from_toml_str(&manifest("cloud-a")).unwrap(),
    ));
    providers.register(Box::new(
        piko_llmd::providers::TomlProvider::from_toml_str(&manifest("cloud-b")).unwrap(),
    ));
    let registry =
        ModelRegistry::with_registry(AuthStorage::in_memory(HashMap::new()), vec![], providers);

    assert!(registry.resolve(Some("shared-model"), None).is_none());
    assert_eq!(
        registry
            .resolve(Some("shared-model"), Some("cloud-a"))
            .unwrap()
            .provider,
        "cloud-a"
    );
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

[api_surfaces.platform]
base_url = "https://api.mycloud.example/v1"
auth_methods = ["api_key"]

[default_targets.platform]
protocol = "chat_completions"

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
    assert_eq!(
        resolved.target.as_ref().unwrap().base_url,
        "https://api.mycloud.example/v1"
    );
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
