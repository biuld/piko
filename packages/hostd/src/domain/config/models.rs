use piko_protocol::{ModelSummary, ProviderAuthMethod, ProviderInfo};

use piko_llmd::auth::AuthStorage;
use piko_llmd::providers::{ModelCatalog, ProviderRegistry};

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub provider: String,
    pub model: ModelSummary,
    pub target: Option<piko_llmd::modeling::ResolvedModelTarget>,
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    auth_storage: AuthStorage,
    scoped_models: Vec<String>,
    catalog: ModelCatalog,
    catalog_dir: Option<std::path::PathBuf>,
}

impl ModelRegistry {
    /// Create a registry from installed provider catalogs in
    /// `$PIKO_HOME/models` (default `~/.piko/models`).
    pub fn new(auth_storage: AuthStorage, scoped_models: Vec<String>) -> Self {
        let mut registry = ProviderRegistry::new();
        let models_dir = piko_models_dir();
        if let Some(models_dir) = &models_dir {
            registry.load_from_dir(models_dir);
        }
        Self {
            auth_storage,
            scoped_models,
            catalog: ModelCatalog::new(registry),
            catalog_dir: models_dir,
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
            catalog_dir: None,
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
                let model_provider = self.find_unique_provider_for_model(&model.id);
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

    fn find_unique_provider_for_model(&self, model_id: &str) -> Option<String> {
        let mut matches = self
            .catalog
            .list_providers()
            .into_iter()
            .filter(|info| info.models.iter().any(|model| model.id == model_id));
        let provider = matches.next()?.provider;
        matches.next().is_none().then_some(provider)
    }

    /// Resolve a model by id + optional provider hint.
    pub fn resolve(
        &self,
        model_id: Option<&str>,
        provider_name: Option<&str>,
    ) -> Option<ResolvedModel> {
        // An explicit composite identity is fail-closed.
        if let (Some(mid), Some(provider)) = (model_id, provider_name) {
            return self
                .find_in_provider(provider, mid)
                .map(|model| self.to_resolved(model, provider));
        }

        // A bare model ID resolves only when its provider is unique.
        if let Some(mid) = model_id {
            let provider = self.find_unique_provider_for_model(mid)?;
            let model = self.find_in_provider(&provider, mid)?;
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
        for (provider, model_id) in [("openai", "gpt-4o")] {
            if let Some(model) = self.find_in_provider(provider, model_id) {
                return Some(self.to_resolved(model, provider));
            }
        }

        // Absolute last resort
        self.catalog.list_providers().into_iter().find_map(|info| {
            info.models
                .first()
                .cloned()
                .map(|model| self.to_resolved(model, &info.provider))
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

    pub fn get_oauth(
        &self,
        provider: &str,
    ) -> Option<std::sync::Arc<dyn piko_llmd::providers::OAuthFlow>> {
        self.catalog.get_oauth(provider)
    }

    pub fn auth_storage(&self) -> &AuthStorage {
        &self.auth_storage
    }

    pub fn auth_storage_mut(&mut self) -> &mut AuthStorage {
        &mut self.auth_storage
    }

    pub fn catalog_dir(&self) -> Option<&std::path::Path> {
        self.catalog_dir.as_deref()
    }

    fn to_resolved(&self, model: ModelSummary, provider: &str) -> ResolvedModel {
        let catalog_provider = self.catalog.provider(provider);
        let auth_method = self
            .auth_storage
            .active_method(provider)
            .unwrap_or(ProviderAuthMethod::ApiKey);
        let target =
            catalog_provider.and_then(|provider| provider.target_for_model(auth_method, &model.id));
        ResolvedModel {
            provider: provider.to_string(),
            model,
            target,
        }
    }
}

/// Returns the explicit development catalog or the installed model directory.
fn piko_models_dir() -> Option<std::path::PathBuf> {
    model_catalog_dir_from(
        std::env::var_os("PIKO_DEV_SOURCE_ROOT").map(std::path::PathBuf::from),
        std::env::var_os("PIKO_HOME").map(std::path::PathBuf::from),
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .map(std::path::PathBuf::from),
    )
}

fn model_catalog_dir_from(
    dev_source_root: Option<std::path::PathBuf>,
    piko_root: Option<std::path::PathBuf>,
    home: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    if let Some(root) = dev_source_root {
        return Some(root.join("packages/llmd/resources/models"));
    }
    if let Some(root) = piko_root {
        return Some(root.join("models"));
    }
    home.map(|root| root.join(".piko/models"))
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn development_catalog_is_independent_from_piko_home() {
        let dir = model_catalog_dir_from(
            Some("checkout".into()),
            Some("user-state".into()),
            Some("home".into()),
        );

        assert_eq!(
            dir,
            Some(std::path::PathBuf::from(
                "checkout/packages/llmd/resources/models"
            ))
        );
    }

    #[test]
    fn installed_catalog_uses_piko_home() {
        let dir = model_catalog_dir_from(None, Some("installation".into()), None);

        assert_eq!(dir, Some(std::path::PathBuf::from("installation/models")));
    }
}
