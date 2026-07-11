use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use piko_protocol::agents::HostTaskContext;
use piko_protocol::messages::ToolCall;
use piko_protocol::tools::{ToolDef, ToolProviderSource};

/// Structured error from a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ToolExecError>,
}

/// Context for tool discovery: which agent/task is discovering tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiscoveryContext {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "taskId")]
    pub task_id: Option<String>,
    #[serde(rename = "toolSetIds")]
    pub tool_set_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "activeToolNames")]
    pub active_tool_names: Option<Vec<String>>,
}

/// Context for tool execution: which agent/task/turn is executing the call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionContext {
    pub agent_id: String,
    pub task_id: String,
    pub tool_set_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_seq: Option<u64>,
    #[serde(skip, default)]
    pub next_event_seq: Option<fn() -> u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_entity_id: Option<String>,
    #[serde(skip)]
    pub host_context: Option<HostTaskContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_work_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
}

/// Interface for tool providers that can discover and execute tools.
#[async_trait]
pub trait ToolProvider: Send + Sync + 'static {
    fn id(&self) -> &str;

    fn source(&self) -> ToolProviderSource;

    async fn discover(&self, context: ToolDiscoveryContext) -> Vec<ToolDef>;

    async fn execute(&self, call: ToolCall, context: ToolExecutionContext) -> ToolExecResult;
}
