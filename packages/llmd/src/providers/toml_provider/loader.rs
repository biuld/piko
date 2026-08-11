// ---- llmd: catalog — TOML-based provider & model catalog loader ----
//
// Provider configs are stored as TOML files under resources/models/.
// Each file defines a [provider] section plus [models.<id>] entries.
// Built-in catalogs are embedded at compile time via include_str!.
// Runtime loading from user-provided paths is supported via TomlProvider::from_toml().

use std::collections::HashMap;
use std::path::Path;

use piko_protocol::model::{
    InferenceDeliveryMode, InputModality, ModelSummary, OutputModality, ProviderAuthMethod,
    ThinkingLevel, ToolExecutionLocus,
};
use serde::Deserialize;

use super::TomlProvider;
use crate::modeling::{
    ApiSurface, ModelTargetProfile, ProtocolProfile, ResponsesContinuationPolicy,
};

// ---- TOML structures ----

#[derive(Debug, Deserialize)]
struct ProviderToml {
    provider: ProviderHeader,
    api_surfaces: HashMap<String, ApiSurfaceToml>,
    default_targets: HashMap<String, TargetToml>,
    #[serde(default)]
    models: HashMap<String, ModelToml>,
}

#[derive(Debug, Deserialize)]
struct ProviderHeader {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ApiSurfaceToml {
    base_url: String,
    auth_methods: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TargetToml {
    protocol: String,
    #[serde(default)]
    continuation: ResponsesContinuationPolicy,
}

#[derive(Debug, Deserialize)]
struct ModelToml {
    name: String,
    reasoning: bool,
    input: Vec<String>,
    context_window: u64,
    max_tokens: u64,
    #[serde(default)]
    targets: HashMap<String, TargetToml>,
    #[serde(default)]
    thinking_level_map: Option<HashMap<String, String>>,
    #[serde(default)]
    pricing: Vec<super::pricing::PricingToml>,
}

// ---- Built-in catalogs (embedded at compile time) ----

const OPENAI_TOML: &str = include_str!("../../../resources/models/openai.toml");
const DEEPSEEK_TOML: &str = include_str!("../../../resources/models/deepseek.toml");

const BUILTIN_PROVIDERS: &[(&str, &str)] = &[("openai", OPENAI_TOML), ("deepseek", DEEPSEEK_TOML)];

// ---- Adapter kind mapping ----

fn parse_protocol(target: &TargetToml) -> Option<ProtocolProfile> {
    match target.protocol.as_str() {
        "chat_completions" => Some(ProtocolProfile::ChatCompletions),
        "responses" => Some(ProtocolProfile::Responses {
            continuation: target.continuation,
        }),
        _ => None,
    }
}

fn parse_auth_method(value: &str) -> Option<ProviderAuthMethod> {
    match value {
        "api_key" => Some(ProviderAuthMethod::ApiKey),
        "oauth" => Some(ProviderAuthMethod::OAuth),
        _ => None,
    }
}

fn parse_thinking_level(s: &str) -> Option<ThinkingLevel> {
    match s {
        "off" => Some(ThinkingLevel::Off),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" => Some(ThinkingLevel::XHigh),
        "max" => Some(ThinkingLevel::Max),
        _ => None,
    }
}

// ---- Parsing ----

fn parse_models(models_toml: HashMap<String, ModelToml>) -> Vec<ModelSummary> {
    models_toml
        .into_iter()
        .map(|(id, m)| {
            let input = m
                .input
                .iter()
                .filter_map(|s| match s.as_str() {
                    "text" => Some(InputModality::Text),
                    "image" => Some(InputModality::Image),
                    _ => None,
                })
                .collect();

            let reasoning_efforts = semantic_reasoning_efforts(&m);

            ModelSummary {
                id,
                name: m.name,
                reasoning: m.reasoning,
                input,
                context_window: m.context_window,
                max_tokens: m.max_tokens,
                reasoning_efforts,
                output: vec![OutputModality::Text],
                tool_execution_loci: vec![ToolExecutionLocus::Caller],
                parallel_tool_calls: true,
                structured_output: false,
                delivery_modes: vec![
                    InferenceDeliveryMode::Streaming,
                    InferenceDeliveryMode::Assembled,
                ],
            }
        })
        .collect()
}

fn semantic_reasoning_efforts(model: &ModelToml) -> Vec<ThinkingLevel> {
    if !model.reasoning {
        return Vec::new();
    }
    let mut efforts = [
        ThinkingLevel::Off,
        ThinkingLevel::Minimal,
        ThinkingLevel::Low,
        ThinkingLevel::Medium,
        ThinkingLevel::High,
        ThinkingLevel::XHigh,
    ]
    .into_iter()
    .filter(|level| {
        model
            .thinking_level_map
            .as_ref()
            .and_then(|map| map.get(level.as_str()))
            .is_none_or(|wire_value| !wire_value.is_empty())
    })
    .collect::<Vec<_>>();
    if model
        .thinking_level_map
        .as_ref()
        .and_then(|map| map.get(ThinkingLevel::Max.as_str()))
        .is_some_and(|wire_value| !wire_value.is_empty())
    {
        efforts.push(ThinkingLevel::Max);
    }
    efforts
}

fn private_reasoning_map(model: &ModelToml) -> std::collections::BTreeMap<ThinkingLevel, String> {
    model
        .thinking_level_map
        .as_ref()
        .into_iter()
        .flat_map(|map| map.iter())
        .filter_map(|(level, wire_value)| {
            (!wire_value.is_empty())
                .then(|| parse_thinking_level(level).map(|level| (level, wire_value.clone())))
                .flatten()
        })
        .collect()
}

fn parse_provider_toml(toml_str: &str) -> Result<ProviderToml, String> {
    toml::from_str(toml_str).map_err(|e| format!("Failed to parse provider TOML: {e}"))
}

fn build_profiles(
    owner: &str,
    targets: &HashMap<String, TargetToml>,
    surfaces: &HashMap<String, ApiSurface>,
) -> Result<HashMap<String, ModelTargetProfile>, String> {
    targets
        .iter()
        .map(|(surface_id, target)| {
            if !surfaces.contains_key(surface_id) {
                return Err(format!(
                    "Target {owner}/{surface_id} references an unknown API surface"
                ));
            }
            let protocol = parse_protocol(target).ok_or_else(|| {
                format!(
                    "Unknown protocol for target {owner}/{surface_id}: {}",
                    target.protocol
                )
            })?;
            Ok((
                surface_id.clone(),
                ModelTargetProfile {
                    api_surface: surface_id.clone(),
                    protocol,
                },
            ))
        })
        .collect()
}

fn validate_unambiguous(
    owner: &str,
    targets: &HashMap<String, ModelTargetProfile>,
    surfaces: &HashMap<String, ApiSurface>,
) -> Result<(), String> {
    for auth_method in [ProviderAuthMethod::ApiKey, ProviderAuthMethod::OAuth] {
        let count = targets
            .values()
            .filter(|target| {
                surfaces
                    .get(&target.api_surface)
                    .is_some_and(|surface| surface.auth_methods.contains(&auth_method))
            })
            .count();
        if count > 1 {
            return Err(format!(
                "Target set {owner} is ambiguous for {auth_method:?}"
            ));
        }
    }
    Ok(())
}

fn build_provider(
    parsed: ProviderToml,
    billing: &crate::billing::BillingRegistry,
) -> Result<TomlProvider, String> {
    let surfaces = parsed
        .api_surfaces
        .into_iter()
        .map(|(id, surface)| {
            let auth_methods = surface
                .auth_methods
                .iter()
                .map(|method| {
                    parse_auth_method(method).ok_or_else(|| {
                        format!("Unknown auth method for API surface {id}: {method}")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if auth_methods.is_empty() {
                return Err(format!("API surface {id} has no authentication method"));
            }
            Ok((
                id.clone(),
                ApiSurface {
                    id,
                    base_url: surface.base_url,
                    auth_methods,
                },
            ))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    if surfaces.is_empty() {
        return Err("Provider has no API surfaces".into());
    }
    let defaults = build_profiles("default", &parsed.default_targets, &surfaces)?;
    validate_unambiguous("default", &defaults, &surfaces)?;

    let mut model_targets = HashMap::new();
    for (model_id, model) in &parsed.models {
        if model.targets.is_empty() {
            if defaults.is_empty() {
                return Err(format!("Model {model_id} has no target profile"));
            }
            continue;
        }
        let targets = build_profiles(model_id, &model.targets, &surfaces)?;
        validate_unambiguous(model_id, &targets, &surfaces)?;
        model_targets.insert(model_id.clone(), targets);
    }
    let reasoning_effort_maps = parsed
        .models
        .iter()
        .map(|(id, model)| (id.clone(), private_reasoning_map(model)))
        .collect();
    let billing = super::pricing::build_pricing(
        parsed
            .models
            .iter()
            .map(|(id, model)| (id.clone(), model.pricing.clone()))
            .collect(),
        &surfaces,
        billing,
    )?;
    let models = parse_models(parsed.models);

    Ok(TomlProvider::new(&parsed.provider.id)
        .with_api_surfaces(surfaces)
        .with_default_targets(defaults)
        .with_models(models)
        .with_reasoning_effort_maps(reasoning_effort_maps)
        .with_billing(billing)
        .with_model_targets(model_targets))
}

// ---- Public API ----

/// Load all built-in providers from embedded TOML catalogs.
pub fn load_builtin_providers() -> Vec<TomlProvider> {
    let billing = crate::billing::BillingRegistry::standard();
    BUILTIN_PROVIDERS
        .iter()
        .filter_map(|(id, toml)| {
            parse_provider_toml(toml)
                .ok()
                .and_then(|parsed| build_provider(parsed, &billing).ok())
                .or_else(|| {
                    tracing::warn!("Failed to load built-in provider: {id}");
                    None
                })
        })
        .collect()
}

/// Load a provider from a TOML file path.
pub fn load_provider_from_path(path: &Path) -> Result<TomlProvider, String> {
    let toml_str = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    load_provider_from_toml(&toml_str)
}

/// Load a provider from a TOML string.
pub fn load_provider_from_toml(toml_str: &str) -> Result<TomlProvider, String> {
    let billing = crate::billing::BillingRegistry::standard();
    load_provider_from_toml_with_billing(toml_str, &billing)
}

/// Load a provider using the same billing plugins that will execute its plans.
pub fn load_provider_from_toml_with_billing(
    toml_str: &str,
    billing: &crate::billing::BillingRegistry,
) -> Result<TomlProvider, String> {
    let parsed = parse_provider_toml(toml_str)?;
    build_provider(parsed, billing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Provider;
    use piko_protocol::model::ModelSummary;
    use piko_protocol::model::ProviderAuthMethod;

    mod pricing_tests;

    fn load_builtin_provider(provider_id: &str) -> Result<TomlProvider, String> {
        let toml_str = BUILTIN_PROVIDERS
            .iter()
            .find(|(id, _)| *id == provider_id)
            .map(|(_, toml)| *toml)
            .ok_or_else(|| format!("No built-in provider: {provider_id}"))?;
        let parsed = parse_provider_toml(toml_str)?;
        build_provider(parsed, &crate::billing::BillingRegistry::standard())
    }

    #[test]
    fn openai_catalog_owns_platform_and_subscription_targets() {
        let provider = load_builtin_provider("openai").unwrap();
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
                continuation: ResponsesContinuationPolicy::EncryptedReasoning
            }
        );
    }

    fn load_models(provider: &str) -> Vec<ModelSummary> {
        BUILTIN_PROVIDERS
            .iter()
            .find(|(id, _)| *id == provider)
            .and_then(|(_, toml)| parse_provider_toml(toml).ok())
            .map(|p| parse_models(p.models))
            .unwrap_or_default()
    }

    #[test]
    fn unsupported_native_provider_is_not_bundled() {
        assert!(load_builtin_provider("anthropic").is_err());
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
    fn test_load_openai_models() {
        let models = load_models("openai");
        assert!(!models.is_empty(), "OpenAI catalog should not be empty");

        let gpt5 = models.iter().find(|m| m.id == "gpt-5").unwrap();
        assert!(gpt5.reasoning);
        assert_eq!(gpt5.context_window, 400000);
    }

    #[test]
    fn test_load_builtin_provider() {
        use crate::providers::Provider;
        let provider = load_builtin_provider("openai").unwrap();
        assert_eq!(provider.id(), "openai");
        let models = provider.list_models();
        assert!(models.len() > 10);
    }
}
