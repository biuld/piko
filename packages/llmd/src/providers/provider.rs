use async_trait::async_trait;

use piko_protocol::model::ModelSummary;

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

    /// API key for this provider, if configured.
    fn api_key(&self) -> Option<&str> {
        None
    }
}
