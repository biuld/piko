// ---- llmd: catalog — TOML-based provider & model catalog loader ----
//
// Provider configs are TOML files. Each file defines a [provider] section plus
// [models.<id>] entries. Production catalogs are loaded from the user
// installation; repository resources are test fixtures only.

use std::collections::HashMap;
use std::path::Path;

use piko_protocol::model::{
    InferenceDeliveryMode, InputModality, ModelSummary, OutputModality, ProviderAuthMethod,
    ThinkingLevel, ToolExecutionLocus,
};
use serde::Deserialize;

use super::TomlProvider;
use crate::modeling::{
    ApiSurface, ModelTargetProfile, ProtocolProfile, ResponsesContinuationPolicy, ResponsesVariant,
};

mod upstream;
use upstream::{
    build_upstream_tools, parse_upstream_kind_set, validate_effective_catalog,
    validate_kind_references,
};

// ---- TOML structures ----

#[derive(Debug, Deserialize)]
struct ProviderToml {
    provider: ProviderHeader,
    #[serde(default)]
    upstream_tools: HashMap<String, UpstreamToolToml>,
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
    #[serde(default)]
    upstream_tools: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct TargetToml {
    protocol: String,
    #[serde(default)]
    continuation: ResponsesContinuationPolicy,
    #[serde(default)]
    variant: ResponsesVariant,
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
    #[serde(default)]
    upstream_tools: Option<Vec<String>>,
    #[serde(default)]
    upstream_tool_overrides: HashMap<String, UpstreamToolToml>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpstreamToolToml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    approval: Option<String>,
    definition: serde_json::Value,
    #[serde(default)]
    choice: Option<serde_json::Value>,
    #[serde(default)]
    activity_types: Vec<String>,
}

// ---- Adapter kind mapping ----

fn parse_protocol(target: &TargetToml) -> Option<ProtocolProfile> {
    match target.protocol.as_str() {
        "chat_completions" => Some(ProtocolProfile::ChatCompletions),
        "responses" => Some(ProtocolProfile::Responses {
            continuation: target.continuation,
            variant: target.variant,
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

fn parse_models(
    models_toml: HashMap<String, ModelToml>,
    provider_upstream_tools: &std::collections::BTreeSet<crate::capabilities::UpstreamToolKind>,
) -> Vec<ModelSummary> {
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

            let supports_upstream = m
                .upstream_tools
                .as_ref()
                .map(|tools| !tools.is_empty())
                .unwrap_or(
                    !provider_upstream_tools.is_empty() || !m.upstream_tool_overrides.is_empty(),
                );
            let mut tool_execution_loci = vec![ToolExecutionLocus::Caller];
            if supports_upstream {
                tool_execution_loci.push(ToolExecutionLocus::Upstream);
            }
            ModelSummary {
                id,
                name: m.name,
                reasoning: m.reasoning,
                input,
                context_window: m.context_window,
                max_tokens: m.max_tokens,
                reasoning_efforts,
                output: vec![OutputModality::Text],
                tool_execution_loci,
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
    let upstream_tools = build_upstream_tools(&parsed.upstream_tools)?;
    let provider_upstream_kinds = upstream_tools.keys().cloned().collect();
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
                    upstream_tools: parse_upstream_kind_set(
                        surface.upstream_tools.as_ref(),
                        "API surface",
                    )?,
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
    let model_upstream_tools = parsed
        .models
        .iter()
        .map(|(id, model)| {
            Ok((
                id.clone(),
                parse_upstream_kind_set(model.upstream_tools.as_ref(), id)?,
            ))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    let model_upstream_tool_overrides = parsed
        .models
        .iter()
        .map(|(id, model)| {
            Ok((
                id.clone(),
                build_upstream_tools(&model.upstream_tool_overrides)?,
            ))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    let all_defined_kinds = upstream_tools
        .keys()
        .chain(
            model_upstream_tool_overrides
                .values()
                .flat_map(|tools| tools.keys()),
        )
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for (surface_id, surface) in &surfaces {
        if let Some(references) = &surface.upstream_tools {
            validate_kind_references(
                &format!("API surface {surface_id}"),
                references,
                &all_defined_kinds,
            )?;
        }
    }
    for model_id in parsed.models.keys() {
        let mut effective = upstream_tools.clone();
        if let Some(overrides) = model_upstream_tool_overrides.get(model_id) {
            effective.extend(overrides.clone());
        }
        if let Some(Some(references)) = model_upstream_tools.get(model_id) {
            validate_kind_references(
                &format!("model {model_id}"),
                references,
                &effective.keys().cloned().collect(),
            )?;
        }
        validate_effective_catalog(&format!("model {model_id}"), &effective)?;
    }
    let models = parse_models(parsed.models, &provider_upstream_kinds);

    Ok(TomlProvider::new(&parsed.provider.id)
        .with_api_surfaces(surfaces)
        .with_default_targets(defaults)
        .with_models(models)
        .with_reasoning_effort_maps(reasoning_effort_maps)
        .with_billing(billing)
        .with_upstream_tools(upstream_tools)
        .with_model_upstream_tools(model_upstream_tools)
        .with_model_upstream_tool_overrides(model_upstream_tool_overrides)
        .with_model_targets(model_targets))
}

// ---- Public API ----

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
mod tests;
