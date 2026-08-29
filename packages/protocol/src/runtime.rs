// ---- Protocol: runtime — run config types ----

use serde::{Deserialize, Serialize};

use super::messages::Message;
use super::model::ModelRunSettings;

// ---- Model config passed by Host ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrchModelConfig {
    pub model: super::messages::Model,
    pub settings: ModelRunSettings,
}

// ---- Runtime-wide scheduling limits ----

/// Default maximum number of AgentInstances in one session tree, including
/// the root AgentInstance.
pub const DEFAULT_MAX_AGENTS: u32 = 32;

/// Default maximum number of agent-tree levels, including the root level.
pub const DEFAULT_MAX_DEPTH: u32 = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorRuntimeConfig {
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "maxConcurrentAgents"
    )]
    pub max_concurrent_agents: Option<u32>,
    /// Maximum AgentInstances in one session tree. `None` uses the runtime
    /// default and is retained for compatibility with older config payloads.
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxAgents")]
    pub max_agents: Option<u32>,
    /// Maximum agent-tree levels, including the root. `None` uses the runtime
    /// default and is retained for compatibility with older config payloads.
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxDepth")]
    pub max_depth: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_limits_use_camel_case_wire_names() {
        let config = OrchestratorRuntimeConfig {
            max_concurrent_agents: None,
            max_agents: Some(5),
            max_depth: Some(2),
        };
        let value = serde_json::to_value(config).expect("serialize runtime config");
        assert_eq!(value["maxAgents"], 5);
        assert_eq!(value["maxDepth"], 2);
    }

    #[test]
    fn runtime_limits_have_stable_defaults() {
        assert_eq!(DEFAULT_MAX_AGENTS, 32);
        assert_eq!(DEFAULT_MAX_DEPTH, 8);
        assert_eq!(OrchestratorRuntimeConfig::default().max_agents, None);
        assert_eq!(OrchestratorRuntimeConfig::default().max_depth, None);
    }

    #[test]
    fn runtime_limits_are_optional_for_older_payloads() {
        let config: OrchestratorRuntimeConfig =
            serde_json::from_value(serde_json::json!({"maxConcurrentAgents": 4}))
                .expect("deserialize legacy runtime config");
        assert_eq!(config.max_concurrent_agents, Some(4));
        assert_eq!(config.max_agents, None);
        assert_eq!(config.max_depth, None);
    }
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
    pub host_context: Option<super::agents::HostSessionContext>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sourceTurnId")]
    pub source_turn_id: Option<String>,
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
