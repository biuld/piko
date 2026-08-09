use piko_protocol::model::{ModelSummary, ProviderAuthMethod};
use std::collections::HashMap;

use super::provider::{Provider, ProviderTarget};

pub(crate) mod loader;

// ============================================================================
// TomlProvider — TOML-configured provider (API key + URL + adapter kind)
// ============================================================================

/// A Provider backed by a TOML manifest. Used for built-in providers
/// (shipped via include_str!) and user-configured custom endpoints.
pub struct TomlProvider {
    id: String,
    protocol: piko_protocol::config::ModelProtocol,
    api_key: Option<String>,
    base_url: Option<String>,
    oauth_target: Option<ProviderTarget>,
    model_targets: HashMap<String, ProviderTarget>,
    models: Vec<ModelSummary>,
}

impl TomlProvider {
    pub fn new(id: impl Into<String>, protocol: piko_protocol::config::ModelProtocol) -> Self {
        Self {
            id: id.into(),
            protocol,
            api_key: None,
            base_url: None,
            oauth_target: None,
            model_targets: HashMap::new(),
            models: Vec::new(),
        }
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn with_oauth_target(mut self, target: ProviderTarget) -> Self {
        self.oauth_target = Some(target);
        self
    }

    pub fn with_models(mut self, models: Vec<ModelSummary>) -> Self {
        self.models = models;
        self
    }

    pub fn with_model_targets(mut self, targets: HashMap<String, ProviderTarget>) -> Self {
        self.model_targets = targets;
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
}

impl Provider for TomlProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn protocol(&self) -> piko_protocol::config::ModelProtocol {
        self.protocol
    }

    fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    fn target(&self, auth_method: ProviderAuthMethod) -> Option<ProviderTarget> {
        match auth_method {
            ProviderAuthMethod::ApiKey => Some(ProviderTarget {
                protocol: self.protocol,
                base_url: self.base_url.clone(),
                responses_continuation: Default::default(),
            }),
            ProviderAuthMethod::OAuth => self.oauth_target.clone(),
        }
    }

    fn target_for_model(
        &self,
        auth_method: ProviderAuthMethod,
        model_id: &str,
    ) -> Option<ProviderTarget> {
        match auth_method {
            ProviderAuthMethod::ApiKey => self
                .model_targets
                .get(model_id)
                .cloned()
                .or_else(|| self.target(auth_method)),
            ProviderAuthMethod::OAuth => self.target(auth_method),
        }
    }

    fn list_models(&self) -> Vec<ModelSummary> {
        self.models.clone()
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }
}
