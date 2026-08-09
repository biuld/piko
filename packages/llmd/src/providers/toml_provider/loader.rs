// ---- llmd: catalog — TOML-based provider & model catalog loader ----
//
// Provider configs are stored as TOML files under resources/models/.
// Each file defines a [provider] section plus [models.<id>] entries.
// Built-in catalogs are embedded at compile time via include_str!.
// Runtime loading from user-provided paths is supported via TomlProvider::from_toml().

use std::collections::HashMap;
use std::path::Path;

use piko_protocol::model::{InputModality, ModelSummary, ThinkingLevel};
use serde::Deserialize;

use super::TomlProvider;
use crate::providers::provider::ProviderTarget;

// ---- TOML structures ----

#[derive(Debug, Deserialize)]
struct ProviderToml {
    provider: ProviderHeader,
    #[serde(default)]
    models: HashMap<String, ModelToml>,
}

#[derive(Debug, Deserialize)]
struct ProviderHeader {
    id: String,
    protocol: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    oauth_target: Option<TargetToml>,
}

#[derive(Debug, Deserialize)]
struct TargetToml {
    protocol: String,
    base_url: String,
    #[serde(default)]
    continuation: piko_protocol::config::ResponsesContinuationPolicy,
}

#[derive(Debug, Deserialize)]
struct ModelToml {
    name: String,
    reasoning: bool,
    input: Vec<String>,
    context_window: u64,
    max_tokens: u64,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    continuation: Option<piko_protocol::config::ResponsesContinuationPolicy>,
    #[serde(default)]
    thinking_level_map: Option<HashMap<String, String>>,
}

// ---- Built-in catalogs (embedded at compile time) ----

const OPENAI_TOML: &str = include_str!("../../../resources/models/openai.toml");
const DEEPSEEK_TOML: &str = include_str!("../../../resources/models/deepseek.toml");

const BUILTIN_PROVIDERS: &[(&str, &str)] = &[("openai", OPENAI_TOML), ("deepseek", DEEPSEEK_TOML)];

// ---- Adapter kind mapping ----

fn parse_protocol(s: &str) -> Option<piko_protocol::config::ModelProtocol> {
    match s {
        "chat_completions" => Some(piko_protocol::config::ModelProtocol::ChatCompletions),
        "responses" => Some(piko_protocol::config::ModelProtocol::Responses),
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

            let thinking_level_map = m.thinking_level_map.map(|map| {
                map.into_iter()
                    .filter_map(|(level_str, value)| {
                        let level = parse_thinking_level(&level_str)?;
                        let mapped = if value.is_empty() { None } else { Some(value) };
                        Some((level, mapped))
                    })
                    .collect()
            });

            ModelSummary {
                id,
                name: m.name,
                reasoning: m.reasoning,
                input,
                context_window: m.context_window,
                max_tokens: m.max_tokens,
                thinking_level_map,
            }
        })
        .collect()
}

fn parse_provider_toml(toml_str: &str) -> Result<ProviderToml, String> {
    toml::from_str(toml_str).map_err(|e| format!("Failed to parse provider TOML: {e}"))
}

fn build_provider(parsed: ProviderToml, api_key: Option<String>) -> Result<TomlProvider, String> {
    let protocol = parse_protocol(&parsed.provider.protocol)
        .ok_or_else(|| format!("Unknown protocol: {}", parsed.provider.protocol))?;

    let model_targets = parsed
        .models
        .iter()
        .filter_map(|(model_id, model)| {
            model.protocol.as_deref().map(|value| {
                let protocol = parse_protocol(value)
                    .ok_or_else(|| format!("Unknown protocol for {model_id}: {value}"))?;
                Ok((
                    model_id.clone(),
                    ProviderTarget {
                        protocol,
                        base_url: parsed.provider.base_url.clone(),
                        responses_continuation: model.continuation.unwrap_or_default(),
                    },
                ))
            })
        })
        .collect::<Result<HashMap<_, _>, String>>()?;

    let models = parse_models(parsed.models);

    let mut provider = TomlProvider::new(&parsed.provider.id, protocol)
        .with_models(models)
        .with_model_targets(model_targets);

    if let Some(base_url) = parsed.provider.base_url {
        provider = provider.with_base_url(base_url);
    }
    if let Some(target) = parsed.provider.oauth_target {
        let protocol = parse_protocol(&target.protocol)
            .ok_or_else(|| format!("Unknown OAuth target protocol: {}", target.protocol))?;
        provider = provider.with_oauth_target(ProviderTarget {
            protocol,
            base_url: Some(target.base_url),
            responses_continuation: target.continuation,
        });
    }
    if let Some(key) = api_key {
        provider = provider.with_api_key(key);
    }

    Ok(provider)
}

// ---- Public API ----

/// Load all built-in providers from embedded TOML catalogs.
pub fn load_builtin_providers() -> Vec<TomlProvider> {
    BUILTIN_PROVIDERS
        .iter()
        .filter_map(|(id, toml)| {
            parse_provider_toml(toml)
                .ok()
                .and_then(|parsed| build_provider(parsed, None).ok())
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
    let parsed = parse_provider_toml(toml_str)?;
    build_provider(parsed, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Provider;
    use piko_protocol::model::ModelSummary;
    use piko_protocol::model::ProviderAuthMethod;

    fn load_builtin_provider(provider_id: &str) -> Result<TomlProvider, String> {
        let toml_str = BUILTIN_PROVIDERS
            .iter()
            .find(|(id, _)| *id == provider_id)
            .map(|(_, toml)| *toml)
            .ok_or_else(|| format!("No built-in provider: {provider_id}"))?;
        let parsed = parse_provider_toml(toml_str)?;
        build_provider(parsed, None)
    }

    #[test]
    fn openai_catalog_owns_platform_and_subscription_targets() {
        let provider = load_builtin_provider("openai").unwrap();
        let platform = provider.target(ProviderAuthMethod::ApiKey).unwrap();
        assert_eq!(
            platform.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
        let subscription = provider.target(ProviderAuthMethod::OAuth).unwrap();
        assert_eq!(
            subscription.base_url.as_deref(),
            Some("https://chatgpt.com/backend-api/codex/")
        );
        assert_eq!(
            subscription.protocol,
            piko_protocol::config::ModelProtocol::Responses
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
    fn deepseek_catalog_selects_protocol_per_model() {
        let provider = load_builtin_provider("deepseek").unwrap();
        let flash = provider
            .target_for_model(ProviderAuthMethod::ApiKey, "deepseek-v4-flash")
            .unwrap();
        assert_eq!(
            flash.protocol,
            piko_protocol::config::ModelProtocol::Responses
        );
        assert_eq!(
            flash.responses_continuation,
            piko_protocol::config::ResponsesContinuationPolicy::StatelessReplay
        );
        assert_eq!(
            provider
                .target_for_model(ProviderAuthMethod::ApiKey, "deepseek-v4-pro")
                .unwrap()
                .protocol,
            piko_protocol::config::ModelProtocol::ChatCompletions
        );
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
