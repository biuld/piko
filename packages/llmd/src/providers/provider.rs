use async_trait::async_trait;

use piko_protocol::config::ModelProtocol;
use piko_protocol::model::{ModelSummary, ProviderAuthMethod};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderTarget {
    pub protocol: ModelProtocol,
    pub base_url: Option<String>,
    pub responses_continuation: piko_protocol::config::ResponsesContinuationPolicy,
}

// ============================================================================
// Provider trait
// ============================================================================

#[async_trait]
pub trait Provider: Send + Sync {
    /// Unique product/authentication provider identifier.
    fn id(&self) -> &str;

    /// Default explicit protocol for targets created from this catalog.
    fn protocol(&self) -> ModelProtocol;

    /// Default API base URL for this provider.
    /// Returns `None` to use llmd's OpenAI Platform default endpoint.
    fn base_url(&self) -> Option<&str> {
        None
    }

    /// Resolve a catalog-owned transport profile for an authentication method.
    fn target(&self, auth_method: ProviderAuthMethod) -> Option<ProviderTarget> {
        matches!(auth_method, ProviderAuthMethod::ApiKey).then(|| ProviderTarget {
            protocol: self.protocol(),
            base_url: self.base_url().map(str::to_owned),
            responses_continuation: Default::default(),
        })
    }

    /// Resolve the explicit target profile for a model. Providers with one
    /// profile inherit `target`; catalogs may override this per model.
    fn target_for_model(
        &self,
        auth_method: ProviderAuthMethod,
        _model_id: &str,
    ) -> Option<ProviderTarget> {
        self.target(auth_method)
    }

    /// List available chat models for this provider.
    fn list_models(&self) -> Vec<ModelSummary>;

    /// API key for this provider, if configured.
    fn api_key(&self) -> Option<&str> {
        None
    }
}
