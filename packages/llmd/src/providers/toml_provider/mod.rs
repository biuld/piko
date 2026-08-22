use piko_protocol::model::{ModelSummary, ProviderAuthMethod};
use std::collections::HashMap;

use super::provider::Provider;
use crate::modeling::{
    ApiSurface, ModelKey, ModelTargetProfile, ResolvedModelTarget, UpstreamToolSupport,
};

pub(crate) mod loader;
mod pricing;

// ============================================================================
// TomlProvider — TOML-configured provider (API key + URL + adapter kind)
// ============================================================================

/// A Provider backed by an installed or project-supplied TOML manifest.
pub struct TomlProvider {
    id: String,
    api_surfaces: HashMap<String, ApiSurface>,
    default_targets: HashMap<String, ModelTargetProfile>,
    model_targets: HashMap<String, HashMap<String, ModelTargetProfile>>,
    models: Vec<ModelSummary>,
    reasoning_effort_maps:
        HashMap<String, std::collections::BTreeMap<piko_protocol::model::ThinkingLevel, String>>,
    billing: HashMap<String, HashMap<String, crate::modeling::BillingPlan>>,
    upstream_tools:
        std::collections::BTreeMap<crate::capabilities::UpstreamToolKind, UpstreamToolSupport>,
    model_upstream_tools:
        HashMap<String, Option<std::collections::BTreeSet<crate::capabilities::UpstreamToolKind>>>,
    model_upstream_tool_overrides: HashMap<
        String,
        std::collections::BTreeMap<crate::capabilities::UpstreamToolKind, UpstreamToolSupport>,
    >,
}

impl TomlProvider {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            api_surfaces: HashMap::new(),
            default_targets: HashMap::new(),
            model_targets: HashMap::new(),
            models: Vec::new(),
            reasoning_effort_maps: HashMap::new(),
            billing: HashMap::new(),
            upstream_tools: Default::default(),
            model_upstream_tools: HashMap::new(),
            model_upstream_tool_overrides: HashMap::new(),
        }
    }

    pub fn with_api_surfaces(mut self, surfaces: HashMap<String, ApiSurface>) -> Self {
        self.api_surfaces = surfaces;
        self
    }

    pub fn with_models(mut self, models: Vec<ModelSummary>) -> Self {
        self.models = models;
        self
    }

    pub fn with_reasoning_effort_maps(
        mut self,
        maps: HashMap<
            String,
            std::collections::BTreeMap<piko_protocol::model::ThinkingLevel, String>,
        >,
    ) -> Self {
        self.reasoning_effort_maps = maps;
        self
    }

    pub fn with_billing(
        mut self,
        billing: HashMap<String, HashMap<String, crate::modeling::BillingPlan>>,
    ) -> Self {
        self.billing = billing;
        self
    }

    pub fn with_default_targets(mut self, targets: HashMap<String, ModelTargetProfile>) -> Self {
        self.default_targets = targets;
        self
    }

    pub fn with_model_targets(
        mut self,
        targets: HashMap<String, HashMap<String, ModelTargetProfile>>,
    ) -> Self {
        self.model_targets = targets;
        self
    }

    pub fn with_upstream_tools(
        mut self,
        tools: std::collections::BTreeMap<
            crate::capabilities::UpstreamToolKind,
            UpstreamToolSupport,
        >,
    ) -> Self {
        self.upstream_tools = tools;
        self
    }

    pub fn with_model_upstream_tools(
        mut self,
        tools: HashMap<
            String,
            Option<std::collections::BTreeSet<crate::capabilities::UpstreamToolKind>>,
        >,
    ) -> Self {
        self.model_upstream_tools = tools;
        self
    }

    pub fn with_model_upstream_tool_overrides(
        mut self,
        tools: HashMap<
            String,
            std::collections::BTreeMap<crate::capabilities::UpstreamToolKind, UpstreamToolSupport>,
        >,
    ) -> Self {
        self.model_upstream_tool_overrides = tools;
        self
    }

    /// Load from a TOML file path.
    pub fn from_toml(path: &std::path::Path) -> Result<Self, String> {
        loader::load_provider_from_path(path)
    }

    /// Load from a TOML string.
    pub fn from_toml_str(toml: &str) -> Result<Self, String> {
        loader::load_provider_from_toml(toml)
    }

    pub fn from_toml_str_with_billing(
        toml: &str,
        billing: &crate::billing::BillingRegistry,
    ) -> Result<Self, String> {
        loader::load_provider_from_toml_with_billing(toml, billing)
    }
}

impl Provider for TomlProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn target_for_model(
        &self,
        auth_method: ProviderAuthMethod,
        model_id: &str,
    ) -> Option<ResolvedModelTarget> {
        let profiles = self
            .model_targets
            .get(model_id)
            .unwrap_or(&self.default_targets);
        let profile = profiles.values().find(|profile| {
            self.api_surfaces
                .get(&profile.api_surface)
                .is_some_and(|surface| surface.auth_methods.contains(&auth_method))
        })?;
        let surface = self.api_surfaces.get(&profile.api_surface)?;
        let model = ModelKey::new(&self.id, model_id);
        let upstream_tools = if matches!(
            profile.protocol,
            crate::modeling::ProtocolProfile::Responses {
                variant: crate::modeling::ResponsesVariant::Standard,
                ..
            }
        ) {
            let mut catalog = self.upstream_tools.clone();
            if let Some(overrides) = self.model_upstream_tool_overrides.get(model_id) {
                catalog.extend(overrides.clone());
            }
            catalog
                .iter()
                .filter(|(kind, _)| {
                    surface
                        .upstream_tools
                        .as_ref()
                        .is_none_or(|allowed| allowed.contains(kind))
                        && self
                            .model_upstream_tools
                            .get(model_id)
                            .and_then(Option::as_ref)
                            .is_none_or(|allowed| allowed.contains(kind))
                })
                .map(|(kind, support)| (kind.clone(), support.clone()))
                .collect()
        } else {
            Default::default()
        };
        Some(ResolvedModelTarget {
            id: format!("{model}@{}", surface.id),
            model,
            api_surface: surface.id.clone(),
            auth_method,
            base_url: surface.base_url.clone(),
            protocol: profile.protocol,
            billing: self
                .billing
                .get(model_id)
                .and_then(|billing| billing.get(&surface.id))
                .cloned(),
            reasoning_effort_map: self
                .reasoning_effort_maps
                .get(model_id)
                .cloned()
                .unwrap_or_default(),
            upstream_tools,
        })
    }

    fn auth_methods(&self) -> Vec<ProviderAuthMethod> {
        let mut methods = Vec::new();
        for method in [ProviderAuthMethod::ApiKey, ProviderAuthMethod::OAuth] {
            let profiles = self.default_targets.values().chain(
                self.model_targets
                    .values()
                    .flat_map(|targets| targets.values()),
            );
            if profiles.into_iter().any(|profile| {
                self.api_surfaces
                    .get(&profile.api_surface)
                    .is_some_and(|surface| surface.auth_methods.contains(&method))
            }) {
                methods.push(method);
            }
        }
        methods
    }

    fn list_models(&self) -> Vec<ModelSummary> {
        self.models.clone()
    }
}
