// ---- Protocol: runtime — run config types ----

use serde::{Deserialize, Serialize};

use super::messages::Message;
use super::model::{ModelProviderConfig, ModelRunSettings};

// ---- Model config passed by Host ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrchModelConfig {
    pub model: super::messages::Model,
    pub provider: ModelProviderConfig,
    pub settings: ModelRunSettings,
    /// Per-model thinking level mapping (from model catalog).
    /// None = use defaults (level.as_str()).
    #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingLevelMap")]
    pub thinking_level_map: crate::model::ThinkingLevelMap,
}

// ---- Runtime-wide scheduling limits ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorRuntimeConfig {
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "maxConcurrentAgents"
    )]
    pub max_concurrent_agents: Option<u32>,
}

// ---- Run options / result ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrchRunCommandOptions {
    #[serde(skip_serializing_if = "Option::is_none", rename = "targetAgentId")]
    pub target_agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrchRunOptions {
    #[serde(flatten)]
    pub command: OrchRunCommandOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "hostContext")]
    pub host_context: Option<super::agents::HostTaskContext>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sourceTurnId")]
    pub source_turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "workId")]
    pub work_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrchRunResult {
    pub messages: Vec<Message>,
    #[serde(rename = "totalSteps")]
    pub total_steps: u32,
    pub status: RunStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Completed,
    Aborted,
    Error,
}

// ---- Graph types ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphSnapshot {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}
