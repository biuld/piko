use std::collections::HashMap;

use piko_protocol::{
    ModelCatalogEntry, ModelProviderConfig, ModelSummary, ProviderInfo, ResolvedModel,
};

use llmd::auth::AuthStorage;

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    auth_storage: AuthStorage,
    scoped_models: Vec<String>,
    custom_providers: HashMap<String, Vec<ModelCatalogEntry>>,
    built_in_models: Vec<ModelCatalogEntry>,
}

impl ModelRegistry {
    pub fn new(auth_storage: AuthStorage, scoped_models: Vec<String>) -> Self {
        Self {
            auth_storage,
            scoped_models,
            custom_providers: HashMap::new(),
            built_in_models: built_in_models(),
        }
    }

    pub fn set_scoped_models(&mut self, patterns: Vec<String>) {
        self.scoped_models = patterns;
    }

    pub fn register_custom_provider(
        &mut self,
        provider_id: impl Into<String>,
        models: Vec<ModelCatalogEntry>,
    ) {
        self.custom_providers.insert(provider_id.into(), models);
    }

    pub fn get_custom_provider_ids(&self) -> Vec<String> {
        self.custom_providers.keys().cloned().collect()
    }

    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        let mut by_provider: HashMap<String, Vec<ModelSummary>> = HashMap::new();
        for model in self.list_models() {
            by_provider
                .entry(model.provider.clone())
                .or_default()
                .push(ModelSummary {
                    id: model.id,
                    name: model.name,
                });
        }
        let mut providers = by_provider
            .into_iter()
            .map(|(provider, models)| ProviderInfo { provider, models })
            .collect::<Vec<_>>();
        providers.sort_by(|a, b| a.provider.cmp(&b.provider));
        providers
    }

    pub fn list_models(&self) -> Vec<ModelCatalogEntry> {
        let mut models = self.built_in_models.clone();
        for custom_models in self.custom_providers.values() {
            models.extend(custom_models.clone());
        }
        models
    }

    pub fn list_scoped_models(&self) -> Vec<ModelCatalogEntry> {
        if self.scoped_models.is_empty() {
            return self.list_models();
        }
        let all_models = self.list_models();
        let mut matching = Vec::new();
        for pattern in &self.scoped_models {
            let (provider, model_id) = pattern
                .split_once('/')
                .map(|(provider, model_id)| (provider, Some(model_id)))
                .unwrap_or((pattern.as_str(), None));
            for model in &all_models {
                let provider_match =
                    provider.is_empty() || model.provider.eq_ignore_ascii_case(provider);
                let model_match = model_id.is_none_or(|model_id| {
                    model.id.to_lowercase().contains(&model_id.to_lowercase())
                });
                if provider_match
                    && model_match
                    && !matching.iter().any(|existing: &ModelCatalogEntry| {
                        existing.provider == model.provider && existing.id == model.id
                    })
                {
                    matching.push(model.clone());
                }
            }
        }
        matching
    }

    pub fn resolve(
        &self,
        model_id: Option<&str>,
        provider_name: Option<&str>,
    ) -> Option<ResolvedModel> {
        let all_models = self.list_models();

        if let (Some(model_id), Some(provider_name)) = (model_id, provider_name) {
            if let Some(model) = all_models
                .iter()
                .find(|model| model.provider == provider_name && model.id == model_id)
            {
                return Some(self.to_resolved(model.clone()));
            }
            if let Some(model) = all_models.iter().find(|model| model.id == model_id) {
                return Some(self.to_resolved(model.clone()));
            }
        } else if let Some(model_id) = model_id
            && let Some(model) = all_models.iter().find(|model| model.id == model_id)
        {
            return Some(self.to_resolved(model.clone()));
        }

        if let Some(provider_name) = provider_name
            && let Some(model) = all_models
                .iter()
                .find(|model| model.provider == provider_name)
        {
            return Some(self.to_resolved(model.clone()));
        }

        for (provider, model_id) in [
            ("anthropic", "claude-sonnet-4-5-20250929"),
            ("openai", "gpt-4o"),
        ] {
            if let Some(model) = all_models
                .iter()
                .find(|model| model.provider == provider && model.id == model_id)
            {
                return Some(self.to_resolved(model.clone()));
            }
        }

        all_models
            .into_iter()
            .next()
            .map(|model| self.to_resolved(model))
    }

    pub fn has_auth(&self, provider: &str) -> bool {
        self.auth_storage.has_auth(provider)
    }

    pub fn auth_storage(&self) -> &AuthStorage {
        &self.auth_storage
    }

    fn to_resolved(&self, model: ModelCatalogEntry) -> ResolvedModel {
        ResolvedModel {
            provider_config: ModelProviderConfig {
                api_key: self.auth_storage.get_api_key(&model.provider),
                base_url: model.base_url.clone(),
                headers: None,
                reasoning: None,
                session_id: None,
                extra: None,
            },
            model,
        }
    }
}

fn built_in_models() -> Vec<ModelCatalogEntry> {
    vec![
        ModelCatalogEntry {
            id: "claude-sonnet-4-5-20250929".to_string(),
            name: "Claude Sonnet 4.5".to_string(),
            api: "anthropic".to_string(),
            provider: "anthropic".to_string(),
            base_url: None,
            reasoning: true,
            input: vec!["text".to_string(), "image".to_string()],
            context_window: 200_000,
            max_tokens: 64_000,
        },
        ModelCatalogEntry {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            api: "openai-responses".to_string(),
            provider: "openai".to_string(),
            base_url: None,
            reasoning: false,
            input: vec!["text".to_string(), "image".to_string()],
            context_window: 128_000,
            max_tokens: 16_384,
        },
    ]
}
