use piko_protocol::model::ModelSummary;

use super::provider::Provider;

pub(crate) mod loader;

// ============================================================================
// TomlProvider — TOML-configured provider (API key + URL + adapter kind)
// ============================================================================

/// A Provider backed by a TOML manifest. Used for built-in providers
/// (shipped via include_str!) and user-configured custom endpoints.
pub struct TomlProvider {
    id: String,
    adapter: genai::adapter::AdapterKind,
    api_key: Option<String>,
    base_url: Option<String>,
    models: Vec<ModelSummary>,
}

impl TomlProvider {
    pub fn new(id: impl Into<String>, adapter: genai::adapter::AdapterKind) -> Self {
        Self {
            id: id.into(),
            adapter,
            api_key: None,
            base_url: None,
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

    pub fn with_models(mut self, models: Vec<ModelSummary>) -> Self {
        self.models = models;
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

    fn adapter_kind(&self) -> genai::adapter::AdapterKind {
        self.adapter
    }

    fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    fn list_models(&self) -> Vec<ModelSummary> {
        self.models.clone()
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }
}
