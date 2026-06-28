// ---- Domain: model step — model spec, config, and continuation state ----

use serde::{Deserialize, Serialize};

pub use piko_protocol::model::{ModelProviderConfig, ModelRunSettings, ThinkingLevelMap};

/// Lightweight model reference (not the full pi-ai Model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub provider: String,
}

/// Configuration for a model step execution.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub model: ModelSpec,
    pub provider: ModelProviderConfig,
    pub settings: ModelRunSettings,
    /// Per-model thinking level mapping (from model catalog).
    /// None = use defaults (level.as_str()).
    #[allow(clippy::type_complexity)]
    pub thinking_level_map: ThinkingLevelMap,
}

impl ModelConfig {
    /// Resolve the thinking level through the model's thinking_level_map.
    /// Returns the provider-specific value to use in API requests.
    pub fn resolve_thinking(&self) -> Option<String> {
        let level = self.settings.thinking_level.as_ref()?;

        if let Some(ref map) = self.thinking_level_map {
            match map.get(level) {
                Some(Some(value)) => return Some(value.clone()),
                Some(None) => return None,
                None => {}
            }
        }

        Some(level.as_str().to_string())
    }
}

/// Continuation state passed between model steps (extracted from engine_state).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelContinuationState {
    pub version: u32,
    pub kind: String,
    pub counters: ModelRuntimeCounters,
}

/// Runtime counters for model execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelRuntimeCounters {
    #[serde(default)]
    pub model_calls: u32,
    #[serde(default)]
    pub tool_calls: u32,
    #[serde(default)]
    pub consecutive_errors: u32,
    #[serde(default)]
    pub started_at: i64,
}

impl ModelContinuationState {
    pub fn extract(raw: Option<&serde_json::Value>) -> Option<Self> {
        raw.and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn ready(counters: ModelRuntimeCounters) -> serde_json::Value {
        serde_json::to_value(Self {
            version: 1,
            kind: "ready".into(),
            counters,
        })
        .unwrap_or_default()
    }
}

impl ModelRuntimeCounters {
    pub fn new() -> Self {
        Self {
            model_calls: 0,
            tool_calls: 0,
            consecutive_errors: 0,
            started_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}
