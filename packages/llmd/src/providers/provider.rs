use async_trait::async_trait;

use crate::modeling::ResolvedModelTarget;
use piko_protocol::model::{ModelSummary, ProviderAuthMethod};

// ============================================================================
// Provider trait
// ============================================================================

#[async_trait]
pub trait Provider: Send + Sync {
    /// Unique product/authentication provider identifier.
    fn id(&self) -> &str;

    /// Resolve the one catalog-owned target compatible with this model and
    /// authentication route. Catalog construction rejects ambiguous routes.
    fn target_for_model(
        &self,
        auth_method: ProviderAuthMethod,
        model_id: &str,
    ) -> Option<ResolvedModelTarget>;

    /// Authentication methods accepted by at least one catalog API surface.
    fn auth_methods(&self) -> Vec<ProviderAuthMethod>;

    /// List available chat models for this provider.
    fn list_models(&self) -> Vec<ModelSummary>;
}
