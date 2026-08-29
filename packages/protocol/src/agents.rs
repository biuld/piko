// ---- Protocol: agents — agent spec & runtime types ----

use serde::{Deserialize, Serialize};

use super::messages::Message;

// ---- Agent types ----

/// Delegation capability for an AgentInstance.
///
/// This is intentionally separate from `AgentSpec::role`: roles select
/// permission policy while kinds select whether the agent may grow the agent
/// tree.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Supervisor,
    #[default]
    /// `leaf` remains accepted when decoding older configuration or snapshots.
    #[serde(alias = "leaf")]
    Worker,
}

impl AgentKind {
    pub fn can_spawn_subagents(self) -> bool {
        matches!(self, Self::Supervisor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    Idle,
    Running,
    Completed,
    Failed,
    Cancelled,
    Closed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSpec {
    pub id: String,
    pub version: String,
    pub provenance: crate::PromptSource,
    pub name: String,
    pub role: String,
    /// Delegation capability. A missing field in a durable legacy snapshot is
    /// treated as supervisor for compatibility; new host TOML defaults to
    /// worker before this DTO is constructed.
    #[serde(default = "legacy_agent_kind")]
    pub kind: AgentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub base_instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Thinking level override for this agent (e.g. "off", "low", "medium", "high").
    /// When None, inherits from the global model config.
    #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingLevel")]
    pub thinking_level: Option<crate::model::ThinkingLevel>,
    #[serde(rename = "toolSetIds")]
    pub tool_set_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "activeToolNames")]
    pub active_tool_names: Option<Vec<String>>,
}

fn legacy_agent_kind() -> AgentKind {
    AgentKind::Supervisor
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeState {
    pub id: String,
    pub spec: AgentSpec,
    pub status: AgentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_agent_instance_id: Option<String>,
    pub transcript: Vec<Message>,
}

// ---- Host session context ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostSessionContext {
    pub session_id: String,
}

impl HostSessionContext {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentKind, AgentSpec};

    #[test]
    fn agent_spec_uses_base_instructions() {
        let spec: AgentSpec = serde_json::from_value(serde_json::json!({
            "id": "main",
            "version": "1",
            "provenance": {"kind": "built-in-agent", "locator": "agents/main"},
            "name": "Main",
            "role": "root",
            "baseInstructions": "durable prompt",
            "toolSetIds": []
        }))
        .unwrap();
        assert_eq!(spec.base_instructions, "durable prompt");
        assert_eq!(spec.kind, AgentKind::Supervisor);

        let serialized = serde_json::to_value(spec).unwrap();
        assert_eq!(serialized["baseInstructions"], "durable prompt");
        assert_eq!(serialized["kind"], "supervisor");
    }

    #[test]
    fn agent_kind_round_trips_and_exposes_spawn_capability() {
        let spec: AgentSpec = serde_json::from_value(serde_json::json!({
            "id": "scout",
            "version": "1",
            "provenance": {"kind": "built-in-agent", "locator": "agents/scout"},
            "name": "Scout",
            "role": "researcher",
            "kind": "worker",
            "baseInstructions": "research",
            "toolSetIds": []
        }))
        .unwrap();
        assert_eq!(spec.kind, AgentKind::Worker);
        assert!(!spec.kind.can_spawn_subagents());
        assert!(AgentKind::Supervisor.can_spawn_subagents());

        let mut legacy = serde_json::to_value(&spec).unwrap();
        legacy["kind"] = serde_json::json!("leaf");
        let legacy: AgentSpec = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.kind, AgentKind::Worker);
    }
}
