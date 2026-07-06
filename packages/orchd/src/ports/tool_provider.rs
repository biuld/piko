// ---- Port: ToolProvider — interface for tool discovery and execution ----
//
// Tool providers are the primary extension point for adding tools to the
// orchestrator. Each provider discovers its tools and executes calls.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::domain::tasks::task::HostTaskContext;
use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{ToolDef, ToolProviderSource};
use crate::domain::tools::result::ToolExecResult;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_context: Option<HostTaskContext>,
    #[serde(skip)]
    pub senders: Option<crate::runtime::dispatch::DispatchSenders>,
}

/// Interface for tool providers that can discover and execute tools.
#[async_trait]
pub trait ToolProvider: Send + Sync + 'static {
    /// Unique identifier for this provider.
    fn id(&self) -> &str;

    /// Classification of this provider's source.
    fn source(&self) -> ToolProviderSource;

    /// Discover all tools available from this provider in the given context.
    async fn discover(&self, context: ToolDiscoveryContext) -> Vec<ToolDef>;

    /// Execute a tool call and return the result.
    async fn execute(&self, call: ToolCall, context: ToolExecutionContext) -> ToolExecResult;
}
