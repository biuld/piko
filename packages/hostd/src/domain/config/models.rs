use piko_protocol::{ModelProviderConfig, ModelSummary, ProviderInfo, ResolvedModel};

use llmd::auth::AuthStorage;
use llmd::providers::{ModelCatalog, ProviderRegistry};

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    auth_storage: AuthStorage,
    scoped_models: Vec<String>,
    catalog: ModelCatalog,
}

impl ModelRegistry {
    /// Create a registry backed by llmd's built-in Provider catalog plus any
    /// user-defined providers found in `~/.piko/models/*.toml`.
    pub fn new(auth_storage: AuthStorage, scoped_models: Vec<String>) -> Self {
        let mut registry = ProviderRegistry::new();
        if let Some(models_dir) = piko_models_dir() {
            registry.load_from_dir(&models_dir);
        }
        Self {
            auth_storage,
            scoped_models,
            catalog: ModelCatalog::new(registry),
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
        }
    }

    pub fn set_scoped_models(&mut self, patterns: Vec<String>) {
        self.scoped_models = patterns;
    }

    /// All known providers from the registry.
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
        infos.sort_by(|a, b| a.provider.cmp(&b.provider));
        infos
    }

    /// All known models from the registry.
    pub fn list_models(&self) -> Vec<ModelSummary> {
        self.catalog.list_models()
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
                let model_provider = self.find_provider_for_model(&model.id);
                let provider_match = provider_filter.is_empty()
                    || model_provider.is_some_and(|p| p.eq_ignore_ascii_case(provider_filter));
                let model_match = model_filter
                    .is_none_or(|mf| model.id.to_lowercase().contains(&mf.to_lowercase()));
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
        for info in self.catalog.list_providers() {
            if info.models.iter().any(|m| m.id == model_id) {
                return Some(info.provider.clone());
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
        if let (Some(mid), Some(provider)) = (model_id, provider_name)
            && let Some(model) = self.find_in_provider(provider, mid)
        {
            return Some(self.to_resolved(model, provider));
        }

        // Priority 2: model_id match across all providers (ignore provider hint)
        if let Some(mid) = model_id
            && let Some(model) = self.catalog.resolve(mid, None)
        {
            let provider = self
                .find_provider_for_model(&model.id)
                .unwrap_or_else(|| "unknown".into());
            return Some(self.to_resolved(model, &provider));
        }

        // Priority 3: provider fallback (first model of provider)
        if let Some(provider) = provider_name
            && let Some(model) = self
                .catalog
                .list_providers()
                .iter()
                .find(|info| info.provider == provider)
                .and_then(|info| info.models.first().cloned())
        {
            return Some(self.to_resolved(model, provider));
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
        self.list_models().into_iter().next().map(|model| {
            let provider = self
                .find_provider_for_model(&model.id)
                .unwrap_or_else(|| "unknown".into());
            self.to_resolved(model, &provider)
        })
    }

    fn find_in_provider(&self, provider: &str, model_id: &str) -> Option<ModelSummary> {
        if let Some(p) = self.catalog.provider(provider)
            && let Some(model) = p.list_models().into_iter().find(|m| m.id == model_id)
        {
            return Some(model);
        }
        None
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

/// Returns `~/.piko/models` if the home directory can be determined.
fn piko_models_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(|home| std::path::PathBuf::from(home).join(".piko").join("models"))
}
