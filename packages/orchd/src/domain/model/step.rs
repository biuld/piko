// ---- Domain: model step — model spec, config, and continuation state ----

use serde::{Deserialize, Serialize};

pub use piko_protocol::model::ModelRunSettings;

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
    pub settings: ModelRunSettings,
    pub context_window: u64,
    pub max_output_tokens: u64,
    /// Per-run transcript policy: max estimated tokens for a single tool
    /// result in the model view (F-05 settings wiring for F-04 truncation).
    pub max_tool_output_tokens: u64,
}

/// Continuation state passed between model steps (extracted from engine_state).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLoopState {
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

impl AgentLoopState {
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
