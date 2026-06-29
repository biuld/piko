use std::sync::Arc;

use piko_protocol::model::ModelSummary;

use super::oauth::OAuthFlow;
use super::provider::Provider;
use super::registry::ProviderRegistry;

// ============================================================================
// ModelCatalog — aggregates model lists from all registered providers
// ============================================================================

#[derive(Debug, Clone)]
pub struct ModelCatalog {
    registry: Arc<ProviderRegistry>,
}

impl ModelCatalog {
    pub fn new(registry: ProviderRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
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
                has_auth: false, // ModelCatalog doesn't know auth; set by hostd ModelRegistry
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
            self.registry
                .iter()
                .find_map(|p| p.list_models().into_iter().find(|m| m.id == model_id))
        }
    }

    /// Get a specific provider by id.
    pub fn provider(&self, id: &str) -> Option<&dyn Provider> {
        self.registry.get(id)
    }

    /// Get the OAuth flow for a provider, if registered.
    pub fn get_oauth(&self, id: &str) -> Option<&dyn OAuthFlow> {
        self.registry.get_oauth(id)
    }
}
