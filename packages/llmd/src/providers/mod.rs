use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use piko_protocol::model::ModelSummary;

use crate::auth::{AuthCredential, AuthError};

pub mod anthropic;
pub mod openai;

// ============================================================================
// OAuthFlow trait
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuthInfo {
    pub device_auth_id: String,
    pub user_code: String,
    pub interval_seconds: u64,
    pub verification_uri: String,
}

#[async_trait]
pub trait OAuthFlow: Send + Sync {
    /// Return the provider ID, e.g. "openai"
    fn provider_id(&self) -> &str;

    // ---- Device Code Flow ----

    /// Request a new device code and user code
    async fn start_device_auth(&self) -> Result<DeviceAuthInfo, AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Device auth not supported for {}", self.provider_id()),
            ),
        })
    }

    /// Poll the provider until the user completes the flow
    async fn poll_device_auth(&self, _info: &DeviceAuthInfo) -> Result<(String, String), AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Device auth not supported for {}", self.provider_id()),
            ),
        })
    }

    /// Exchange the authorization code for final tokens
    async fn exchange_code(
        &self,
        _code: String,
        _verifier: String,
    ) -> Result<AuthCredential, AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Code exchange not supported for {}", self.provider_id()),
            ),
        })
    }

    // ---- Refresh Flow ----

    /// Refresh an expired token
    async fn refresh_token(&self, _refresh_token: &str) -> Result<AuthCredential, AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Token refresh not supported for {}", self.provider_id()),
            ),
        })
    }
}

// ============================================================================
// Provider trait
// ============================================================================

#[async_trait]
pub trait Provider: Send + Sync {
    /// Unique provider identifier (e.g. "openai", "anthropic").
    fn id(&self) -> &str;

    /// Corresponding genai adapter kind for HTTP dispatch.
    fn adapter_kind(&self) -> genai::adapter::AdapterKind;

    /// Default API base URL for this provider.
    /// Returns None to use genai's built-in default endpoint.
    fn base_url(&self) -> Option<&str> {
        None
    }

    /// List available chat models for this provider.
    fn list_models(&self) -> Vec<ModelSummary>;

    /// If this provider supports OAuth device-code flow, return the flow handle.
    fn oauth(&self) -> Option<&dyn OAuthFlow> {
        None
    }

    /// API key for this provider, if configured.
    fn api_key(&self) -> Option<&str> {
        None
    }
}

// ============================================================================
// ConfigurableProvider — user-configured provider (custom API key + URL + kind)
// ============================================================================

/// A Provider backed by user-supplied configuration rather than a built-in
/// provider implementation. Used for custom OpenAI-compatible endpoints or
/// self-hosted models.
pub struct ConfigurableProvider {
    id: String,
    adapter: genai::adapter::AdapterKind,
    api_key: Option<String>,
    base_url: Option<String>,
    /// User-supplied model catalog entries
    models: Vec<ModelSummary>,
}

impl ConfigurableProvider {
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

    /// Set the model catalog entries for this provider.
    pub fn with_models(mut self, models: Vec<ModelSummary>) -> Self {
        self.models = models;
        self
    }
}

impl Provider for ConfigurableProvider {
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

// ============================================================================
// ProviderRegistry — holds Provider instances, keyed by id
// ============================================================================

pub struct ProviderRegistry {
    providers: std::collections::HashMap<String, Box<dyn Provider>>,
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRegistry")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            providers: std::collections::HashMap::new(),
        };
        registry.register(Box::new(openai::OpenAIProvider::new()));
        registry.register(Box::new(anthropic::AnthropicProvider::new()));
        registry
    }

    pub fn register(&mut self, provider: Box<dyn Provider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn get(&self, id: &str) -> Option<&dyn Provider> {
        self.providers.get(id).map(|p| p.as_ref())
    }

    /// Iterate over all registered providers.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Provider> {
        self.providers.values().map(|p| p.as_ref())
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ModelCatalog — aggregates model lists from all registered providers
// ============================================================================

#[derive(Debug, Clone)]
pub struct ModelCatalog {
    registry: std::sync::Arc<ProviderRegistry>,
}

impl ModelCatalog {
    pub fn new(registry: ProviderRegistry) -> Self {
        Self {
            registry: std::sync::Arc::new(registry),
        }
    }

    /// List all models across all registered providers.
    pub fn list_models(&self) -> Vec<ModelSummary> {
        self.registry.iter().flat_map(|p| p.list_models()).collect()
    }

    /// List all providers with their model summaries.
    pub fn list_providers(&self) -> Vec<piko_protocol::model::ProviderInfo> {
        self.registry
            .iter()
            .map(|p| piko_protocol::model::ProviderInfo {
                provider: p.id().to_string(),
                models: p.list_models(),
            })
            .collect()
    }

    /// Resolve a model by id + optional provider. Falls back to first match.
    pub fn resolve(&self, model_id: &str, provider_name: Option<&str>) -> Option<ModelSummary> {
        if let Some(provider_name) = provider_name {
            if let Some(p) = self.registry.get(provider_name) {
                return p.list_models().into_iter().find(|m| m.id == model_id);
            }
            None
        } else {
            self.registry.iter().find_map(|p| {
                p.list_models().into_iter().find(|m| m.id == model_id)
            })
        }
    }

    /// Get a specific provider by id.
    pub fn provider(&self, id: &str) -> Option<&dyn Provider> {
        self.registry.get(id)
    }
}
