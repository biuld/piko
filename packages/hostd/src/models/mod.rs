use std::collections::HashMap;

use piko_protocol::{
    ModelProviderConfig, ModelSummary, ProviderInfo, ResolvedModel,
};

use llmd::auth::AuthStorage;
use llmd::providers::{ModelCatalog, ProviderRegistry};

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    auth_storage: AuthStorage,
    scoped_models: Vec<String>,
    catalog: ModelCatalog,
    /// provider_id → custom models from user config
    custom_providers: HashMap<String, Vec<ModelSummary>>,
}

impl ModelRegistry {
    /// Create a registry backed by llmd's built-in Provider catalog.
    pub fn new(auth_storage: AuthStorage, scoped_models: Vec<String>) -> Self {
        let builtin_registry = ProviderRegistry::new();
        Self {
            auth_storage,
            scoped_models,
            catalog: ModelCatalog::new(builtin_registry),
            custom_providers: HashMap::new(),
        }
    }

    /// Create a registry with an externally-provided ProviderRegistry (for testing).
    pub fn with_registry(
        auth_storage: AuthStorage,
        scoped_models: Vec<String>,
        registry: ProviderRegistry,
    ) -> Self {
        Self {
            auth_storage,
            scoped_models,
            catalog: ModelCatalog::new(registry),
            custom_providers: HashMap::new(),
        }
    }

    pub fn set_scoped_models(&mut self, patterns: Vec<String>) {
        self.scoped_models = patterns;
    }

    /// Register a user-configured custom provider with its own models.
    pub fn register_custom_provider(
        &mut self,
        provider_id: impl Into<String>,
        models: Vec<ModelSummary>,
    ) {
        self.custom_providers.insert(provider_id.into(), models);
    }

    pub fn get_custom_provider_ids(&self) -> Vec<String> {
        self.custom_providers.keys().cloned().collect()
    }

    /// All known providers: built-in from llmd + custom user-configured.
    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        let mut infos: Vec<ProviderInfo> = self
            .catalog
            .list_providers()
            .into_iter()
            .map(|info| ProviderInfo {
                has_auth: self.auth_storage.has_auth(&info.provider),
                ..info
            })
            .collect();

        // Append custom providers
        for (provider, models) in &self.custom_providers {
            let existing = infos.iter().any(|info| info.provider == *provider);
            if !existing {
                infos.push(ProviderInfo {
                    provider: provider.clone(),
                    models: models.clone(),
                    has_auth: self.auth_storage.has_auth(provider),
                });
            }
        }

        infos.sort_by(|a, b| a.provider.cmp(&b.provider));
        infos
    }

    /// All known models: built-in + custom.
    pub fn list_models(&self) -> Vec<ModelSummary> {
        let mut models = self.catalog.list_models();
        for custom_models in self.custom_providers.values() {
            models.extend(custom_models.clone());
        }
        models
    }

    /// Models matching the configured scope filter.
    pub fn list_scoped_models(&self) -> Vec<ModelSummary> {
        if self.scoped_models.is_empty() {
            return self.list_models();
        }
        let all_models = self.list_models();
        let mut matching = Vec::new();
        for pattern in &self.scoped_models {
            let (provider_filter, model_filter) = pattern
                .split_once('/')
                .map(|(p, m)| (p, Some(m)))
                .unwrap_or((pattern.as_str(), None));

            for model in &all_models {
                // Find which provider this model belongs to
                let model_provider = self.find_provider_for_model(&model.id);
                let provider_match = provider_filter.is_empty()
                    || model_provider.is_some_and(|p| p.eq_ignore_ascii_case(provider_filter));
                let model_match = model_filter.is_none_or(|mf| {
                    model.id.to_lowercase().contains(&mf.to_lowercase())
                });
                if provider_match
                    && model_match
                    && !matching.iter().any(|m: &ModelSummary| m.id == model.id)
                {
                    matching.push(model.clone());
                }
            }
        }
        matching
    }

    fn find_provider_for_model(&self, model_id: &str) -> Option<String> {
        // Check built-in catalog
        for info in self.catalog.list_providers() {
            if info.models.iter().any(|m| m.id == model_id) {
                return Some(info.provider.clone());
            }
        }
        // Check custom providers
        for (provider, models) in &self.custom_providers {
            if models.iter().any(|m| m.id == model_id) {
                return Some(provider.clone());
            }
        }
        None
    }

    /// Resolve a model by id + optional provider hint.
    pub fn resolve(
        &self,
        model_id: Option<&str>,
        provider_name: Option<&str>,
    ) -> Option<ResolvedModel> {
        // Priority 1: exact provider + model match
        if let (Some(mid), Some(provider)) = (model_id, provider_name) {
            if let Some(model) = self.find_in_provider(provider, mid) {
                return Some(self.to_resolved(model, provider));
            }
        }

        // Priority 2: model_id match across all providers (ignore provider hint)
        if let Some(mid) = model_id {
            if let Some(model) = self.catalog.resolve(mid, None) {
                let provider = self
                    .find_provider_for_model(&model.id)
                    .unwrap_or_else(|| "unknown".into());
                return Some(self.to_resolved(model, &provider));
            }
            // Check custom providers
            for (provider, models) in &self.custom_providers {
                if let Some(model) = models.iter().find(|m| m.id == mid) {
                    return Some(self.to_resolved(model.clone(), provider));
                }
            }
        }

        // Priority 3: provider fallback (first model of provider)
        if let Some(provider) = provider_name {
            // Check built-in
            if let Some(model) = self.catalog.list_providers()
                .iter()
                .find(|info| info.provider == provider)
                .and_then(|info| info.models.first().cloned())
            {
                return Some(self.to_resolved(model, provider));
            }
            // Check custom
            if let Some(model) =
                self.custom_providers.get(provider).and_then(|m| m.first())
            {
                return Some(self.to_resolved(model.clone(), provider));
            }
        }

        // Priority 4: hard fallback — first available model
        for (provider, model_id) in [
            ("anthropic", "claude-sonnet-4-5-20250929"),
            ("openai", "gpt-4o"),
        ] {
            if let Some(model) = self.find_in_provider(provider, model_id) {
                return Some(self.to_resolved(model, provider));
            }
        }

        // Absolute last resort
        self.list_models()
            .into_iter()
            .next()
            .map(|model| {
                let provider = self
                    .find_provider_for_model(&model.id)
                    .unwrap_or_else(|| "unknown".into());
                self.to_resolved(model, &provider)
            })
    }

    fn find_in_provider(&self, provider: &str, model_id: &str) -> Option<ModelSummary> {
        // Check built-in
        if let Some(p) = self.catalog.provider(provider) {
            if let Some(model) = p.list_models().into_iter().find(|m| m.id == model_id) {
                return Some(model);
            }
        }
        // Check custom
        self.custom_providers
            .get(provider)
            .and_then(|models| models.iter().find(|m| m.id == model_id))
            .cloned()
    }

    pub fn has_auth(&self, provider: &str) -> bool {
        self.auth_storage.has_auth(provider)
    }

    pub fn get_oauth(&self, provider: &str) -> Option<&dyn llmd::providers::OAuthFlow> {
        self.catalog.get_oauth(provider)
    }

    pub fn auth_storage(&self) -> &AuthStorage {
        &self.auth_storage
    }

    pub fn auth_storage_mut(&mut self) -> &mut AuthStorage {
        &mut self.auth_storage
    }

    fn to_resolved(&self, model: ModelSummary, provider: &str) -> ResolvedModel {
        ResolvedModel {
            provider: provider.to_string(),
            model,
            provider_config: ModelProviderConfig {
                api_key: self.auth_storage.get_api_key(provider),
                base_url: None,
                headers: None,
                reasoning: None,
                session_id: None,
                extra: None,
            },
        }
    }
}
