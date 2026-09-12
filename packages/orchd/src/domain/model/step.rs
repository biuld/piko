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
