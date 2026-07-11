// ---- Protocol: agents — agent & task types ----

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
    pub name: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "systemPrompt")]
    pub system_prompt: String,
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
    pub active_task_id: Option<String>,
    pub transcript: Vec<Message>,
}

// ---- Task types ----

pub type AgentTaskId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TaskSource {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "agent")]
    Agent {
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AgentTaskStatus {
    Queued,
    Running,
    Idle,
    Closed,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTask {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<AgentTaskId>,
    #[serde(rename = "targetAgentId")]
    pub target_agent_id: String,
    pub prompt: String,
    pub source: TaskSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "parentTaskId")]
    pub parent_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "hostContext")]
    pub host_context: Option<HostTaskContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostTaskContext {
    pub session_id: String,
}

impl HostTaskContext {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskState {
    pub id: AgentTaskId,
    #[serde(rename = "targetAgentId")]
    pub target_agent_id: String,
    pub prompt: String,
    pub source: TaskSource,
    pub status: AgentTaskStatus,
    pub priority: i32,
    #[serde(skip_serializing_if = "Option::is_none", rename = "parentTaskId")]
    pub parent_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<AgentTaskResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentArtifact {
    pub id: String,
    #[serde(rename = "type")]
    pub artifact_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskResult {
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<AgentArtifact>>,
}
