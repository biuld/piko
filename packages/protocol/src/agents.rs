// ---- Protocol: agents — agent spec & runtime types ----

use serde::{Deserialize, Serialize};

use super::messages::Message;

// ---- Agent types ----

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
    use super::AgentSpec;

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

        let serialized = serde_json::to_value(spec).unwrap();
        assert_eq!(serialized["baseInstructions"], "durable prompt");
    }
}
