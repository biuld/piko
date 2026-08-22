use super::*;
use crate::providers::Provider;
use piko_protocol::model::{ModelSummary, ProviderAuthMethod};

mod pricing_tests;

fn load_fixture_provider(provider_id: &str) -> Result<TomlProvider, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models")
        .join(format!("{provider_id}.toml"));
    load_provider_from_path(&path)
}

#[test]
fn openai_catalog_owns_platform_and_subscription_targets() {
    let provider = load_fixture_provider("openai").unwrap();
    let platform = provider
        .target_for_model(ProviderAuthMethod::ApiKey, "gpt-5.5")
        .unwrap();
    assert_eq!(platform.api_surface, "platform");
    assert_eq!(platform.base_url, "https://api.openai.com/v1");
    let subscription = provider
        .target_for_model(ProviderAuthMethod::OAuth, "gpt-5.5")
        .unwrap();
    assert_eq!(subscription.api_surface, "subscription");
    assert_eq!(
        subscription.base_url,
        "https://chatgpt.com/backend-api/codex/"
    );
    assert_eq!(
        subscription.protocol,
        ProtocolProfile::Responses {
            continuation: ResponsesContinuationPolicy::EncryptedReasoning,
            variant: ResponsesVariant::Standard,
        }
    );

    for model in ["gpt-5.6", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"] {
        let platform = provider
            .target_for_model(ProviderAuthMethod::ApiKey, model)
            .unwrap();
        assert_eq!(
            platform.protocol,
            ProtocolProfile::Responses {
                continuation: ResponsesContinuationPolicy::PreviousResponseId,
                variant: ResponsesVariant::Standard,
            }
        );
        let subscription = provider
            .target_for_model(ProviderAuthMethod::OAuth, model)
            .unwrap();
        assert_eq!(
            subscription.protocol,
            ProtocolProfile::Responses {
                continuation: ResponsesContinuationPolicy::EncryptedReasoning,
                variant: ResponsesVariant::CodexLite,
            }
        );
    }
}

fn load_models(provider: &str) -> Vec<ModelSummary> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models")
        .join(format!("{provider}.toml"));
    std::fs::read_to_string(path)
        .ok()
        .and_then(|toml| parse_provider_toml(&toml).ok())
        .map(|parsed| {
            let kinds = build_upstream_tools(&parsed.upstream_tools)
                .map(|tools| tools.keys().cloned().collect())
                .unwrap_or_default();
            parse_models(parsed.models, &kinds)
        })
        .unwrap_or_default()
}

#[test]
fn unsupported_native_provider_is_not_bundled() {
    assert!(load_fixture_provider("anthropic").is_err());
}

#[test]
fn ambiguous_auth_routes_are_rejected() {
    let manifest = r#"
[provider]
id = "ambiguous"
[api_surfaces.one]
base_url = "https://one.example"
auth_methods = ["api_key"]
[api_surfaces.two]
base_url = "https://two.example"
auth_methods = ["api_key"]
[default_targets.one]
protocol = "responses"
[default_targets.two]
protocol = "chat_completions"
"#;
    let error = load_provider_from_toml(manifest).err().unwrap();
    assert!(error.contains("ambiguous for ApiKey"));
}

#[test]
fn upstream_tools_resolve_by_provider_surface_and_model() {
    let manifest = r#"
[provider]
id = "fixture"
[upstream_tools.search]
name = "web_search"
approval = "never"
definition = { type = "web_search" }
activity_types = ["web_search_call"]
[upstream_tools.future_media]
name = "future_media"
approval = "on_request"
definition = { type = "future_media", quality = "medium" }
activity_types = ["future_media_call"]
[api_surfaces.platform]
base_url = "https://api.example/v1"
auth_methods = ["api_key"]
upstream_tools = ["search", "future_media"]
[api_surfaces.subscription]
base_url = "https://subscription.example"
auth_methods = ["oauth"]
upstream_tools = ["search"]
[default_targets.platform]
protocol = "responses"
[default_targets.subscription]
protocol = "responses"
variant = "codex_lite"
[models.enabled]
name = "Enabled"
reasoning = true
input = ["text"]
context_window = 1000
max_tokens = 100
[models.enabled.upstream_tool_overrides.future_media]
name = "future_media"
approval = "on_request"
definition = { type = "future_media", quality = "high" }
activity_types = ["future_media_call"]
[models.disabled]
name = "Disabled"
reasoning = true
input = ["text"]
context_window = 1000
max_tokens = 100
upstream_tools = []
"#;
    let provider = load_provider_from_toml(manifest).unwrap();
    let platform = provider
        .target_for_model(ProviderAuthMethod::ApiKey, "enabled")
        .unwrap();
    assert!(
        platform
            .upstream_tools
            .contains_key(&crate::capabilities::UpstreamToolKind::new("search").unwrap())
    );
    assert_eq!(
        platform.upstream_tools
            [&crate::capabilities::UpstreamToolKind::new("future_media").unwrap()]
            .wire_definition,
        serde_json::json!({"type":"future_media", "quality":"high"})
    );
    let lite = provider
        .target_for_model(ProviderAuthMethod::OAuth, "enabled")
        .unwrap();
    assert!(lite.upstream_tools.is_empty());
    let disabled = provider
        .target_for_model(ProviderAuthMethod::ApiKey, "disabled")
        .unwrap();
    assert!(disabled.upstream_tools.is_empty());
}

#[test]
fn upstream_catalog_rejects_undefined_surface_references() {
    let manifest = r#"
[provider]
id = "fixture"
[api_surfaces.platform]
base_url = "https://api.example/v1"
auth_methods = ["api_key"]
upstream_tools = ["future_tool"]
[default_targets.platform]
protocol = "responses"
"#;
    let error = load_provider_from_toml(manifest).err().unwrap();
    assert!(error.contains("references undefined upstream tool kind future_tool"));
}

#[test]
fn upstream_catalog_rejects_invalid_wire_definitions() {
    let manifest = r#"
[provider]
id = "fixture"
[upstream_tools.future_tool]
definition = "not-an-object"
[api_surfaces.platform]
base_url = "https://api.example/v1"
auth_methods = ["api_key"]
[default_targets.platform]
protocol = "responses"
"#;
    let error = load_provider_from_toml(manifest).err().unwrap();
    assert!(error.contains("definition must be an object with a string type"));
}

#[test]
fn upstream_catalog_rejects_ambiguous_activity_ownership() {
    let manifest = r#"
[provider]
id = "fixture"
[upstream_tools.future_a]
definition = { type = "future_a" }
activity_types = ["future_call"]
[upstream_tools.future_b]
definition = { type = "future_b" }
activity_types = ["future_call"]
[api_surfaces.platform]
base_url = "https://api.example/v1"
auth_methods = ["api_key"]
[default_targets.platform]
protocol = "responses"
"#;
    let error = load_provider_from_toml(manifest).err().unwrap();
    assert!(error.contains("uniquely owned"));
}

#[test]
fn test_load_openai_models() {
    let models = load_models("openai");
    assert!(!models.is_empty(), "OpenAI catalog should not be empty");

    let gpt5 = models.iter().find(|m| m.id == "gpt-5").unwrap();
    assert!(gpt5.reasoning);
    assert_eq!(gpt5.context_window, 400000);
}

#[test]
fn test_load_builtin_provider() {
    let provider = load_fixture_provider("openai").unwrap();
    assert_eq!(provider.id(), "openai");
    let models = provider.list_models();
    assert!(models.len() > 10);
}
