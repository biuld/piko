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
    adapter: String,
    #[serde(default)]
    base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelToml {
    name: String,
    reasoning: bool,
    input: Vec<String>,
    context_window: u64,
    max_tokens: u64,
    #[serde(default)]
    thinking_level_map: Option<HashMap<String, String>>,
}

// ---- Built-in catalogs (embedded at compile time) ----

const ANTHROPIC_TOML: &str = include_str!("../../../resources/models/anthropic.toml");
const OPENAI_TOML: &str = include_str!("../../../resources/models/openai.toml");
const DEEPSEEK_TOML: &str = include_str!("../../../resources/models/deepseek.toml");

const BUILTIN_PROVIDERS: &[(&str, &str)] = &[
    ("anthropic", ANTHROPIC_TOML),
    ("openai", OPENAI_TOML),
    ("deepseek", DEEPSEEK_TOML),
];

// ---- Adapter kind mapping ----

fn parse_adapter_kind(s: &str) -> Option<genai::adapter::AdapterKind> {
    match s {
        "anthropic" => Some(genai::adapter::AdapterKind::Anthropic),
        "openai" => Some(genai::adapter::AdapterKind::OpenAI),
        "openai_resp" => Some(genai::adapter::AdapterKind::OpenAIResp),
        "gemini" => Some(genai::adapter::AdapterKind::Gemini),
        "deepseek" => Some(genai::adapter::AdapterKind::DeepSeek),
        "groq" => Some(genai::adapter::AdapterKind::Groq),
        "cohere" => Some(genai::adapter::AdapterKind::Cohere),
        "together" => Some(genai::adapter::AdapterKind::Together),
        "fireworks" => Some(genai::adapter::AdapterKind::Fireworks),
        "ollama" => Some(genai::adapter::AdapterKind::Ollama),
        "vertex" => Some(genai::adapter::AdapterKind::Vertex),
        "github_copilot" => Some(genai::adapter::AdapterKind::GithubCopilot),
        "open_router" => Some(genai::adapter::AdapterKind::OpenRouter),
        "xai" => Some(genai::adapter::AdapterKind::Xai),
        "moonshot" => Some(genai::adapter::AdapterKind::Moonshot),
        "bedrock_api" => Some(genai::adapter::AdapterKind::BedrockApi),
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
    let adapter = parse_adapter_kind(&parsed.provider.adapter)
        .ok_or_else(|| format!("Unknown adapter: {}", parsed.provider.adapter))?;

    let models = parse_models(parsed.models);

    let mut provider = TomlProvider::new(&parsed.provider.id, adapter).with_models(models);

    if let Some(base_url) = parsed.provider.base_url {
        provider = provider.with_base_url(base_url);
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
    use piko_protocol::model::ModelSummary;

    fn load_builtin_provider(provider_id: &str) -> Result<TomlProvider, String> {
        let toml_str = BUILTIN_PROVIDERS
            .iter()
            .find(|(id, _)| *id == provider_id)
            .map(|(_, toml)| *toml)
            .ok_or_else(|| format!("No built-in provider: {provider_id}"))?;
        let parsed = parse_provider_toml(toml_str)?;
        build_provider(parsed, None)
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
    fn test_load_anthropic_models() {
        let models = load_models("anthropic");
        assert!(!models.is_empty(), "Anthropic catalog should not be empty");

        let sonnet45 = models.iter().find(|m| m.id == "claude-sonnet-4-5-20250929");
        assert!(sonnet45.is_some(), "Sonnet 4.5 should exist");
        let s = sonnet45.unwrap();
        assert!(s.reasoning);
        assert_eq!(s.context_window, 200000);
        assert_eq!(s.max_tokens, 64000);
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
        let provider = load_builtin_provider("anthropic").unwrap();
        assert_eq!(provider.id(), "anthropic");
        let models = provider.list_models();
        assert!(models.len() > 10);
    }
}
