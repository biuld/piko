use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::auth::AuthStorage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub context_window: u64,
    pub max_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRunSettings {
    pub parallel_tools: bool,
    pub allow_tool_calls: bool,
    pub runtime_limits: RuntimeLimits,
}

impl Default for ModelRunSettings {
    fn default() -> Self {
        Self {
            parallel_tools: true,
            allow_tool_calls: true,
            runtime_limits: RuntimeLimits {
                per_tool_timeout_ms: 120_000,
                max_consecutive_errors: 5,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLimits {
    pub per_tool_timeout_ms: u64,
    pub max_consecutive_errors: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderInfo {
    pub provider: String,
    pub models: Vec<ModelSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedModel {
    pub model: Model,
    pub provider_config: ModelProviderConfig,
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    auth_storage: AuthStorage,
    scoped_models: Vec<String>,
    custom_providers: HashMap<String, Vec<Model>>,
    built_in_models: Vec<Model>,
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

    pub fn register_custom_provider(&mut self, provider_id: impl Into<String>, models: Vec<Model>) {
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

    pub fn list_models(&self) -> Vec<Model> {
        let mut models = self.built_in_models.clone();
        for custom_models in self.custom_providers.values() {
            models.extend(custom_models.clone());
        }
        models
    }

    pub fn list_scoped_models(&self) -> Vec<Model> {
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
                    && !matching.iter().any(|existing: &Model| {
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

    fn to_resolved(&self, model: Model) -> ResolvedModel {
        ResolvedModel {
            provider_config: ModelProviderConfig {
                api_key: self.auth_storage.get_api_key(&model.provider),
                base_url: model.base_url.clone(),
                headers: None,
            },
            model,
        }
    }
}

pub fn create_default_settings(overrides: Option<ModelRunSettings>) -> ModelRunSettings {
    overrides.unwrap_or_default()
}

fn built_in_models() -> Vec<Model> {
    vec![
        Model {
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
        Model {
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
