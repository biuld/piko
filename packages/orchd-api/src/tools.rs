use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use piko_protocol::agents::{AgentKind, HostSessionContext};
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

/// Context for tool discovery: which agent instance is discovering tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiscoveryContext {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    /// Trusted AgentSpec delegation mode. Missing serialized context fails
    /// closed to `Worker`.
    #[serde(default)]
    pub agent_kind: AgentKind,
    #[serde(skip_serializing_if = "Option::is_none", rename = "agentInstanceId")]
    pub agent_instance_id: Option<String>,
    #[serde(rename = "toolSetIds")]
    pub tool_set_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "activeToolNames")]
    pub active_tool_names: Option<Vec<String>>,
}

/// Context for tool execution: which agent instance/execution/turn is executing the call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionContext {
    /// Trusted runtime identity; never accepted from model-controlled args.
    pub session_id: String,
    pub agent_instance_id: String,
    pub root_input_id: String,
    #[serde(skip, default)]
    pub cancellation: Option<tokio_util::sync::CancellationToken>,
    pub agent_id: String,
    /// F-19: role of the executing agent from the registered `AgentSpec`.
    /// Identity metadata used by role-aware providers (e.g. per-role
    /// sandbox policy); absent roles use the provider's session default.
    #[serde(rename = "agentRole", skip_serializing_if = "Option::is_none")]
    pub agent_role: Option<String>,
    /// Trusted AgentSpec delegation mode. Missing serialized context fails
    /// closed to `Worker`.
    #[serde(default)]
    pub agent_kind: AgentKind,
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
    pub host_context: Option<HostSessionContext>,
    /// Estimated tokens remaining in the context window for the current
    /// model step (F-04 budget basis). Populated by the runtime before a
    /// tool executes; `None` when the window is not resolvable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_remaining: Option<u64>,
}

/// Interface for tool providers that can discover and execute tools.
#[async_trait]
pub trait ToolProvider: Send + Sync + 'static {
    fn id(&self) -> &str;

    fn source(&self) -> ToolProviderSource;

    async fn discover(&self, context: ToolDiscoveryContext) -> Vec<ToolDef>;

    async fn execute(&self, call: ToolCall, context: ToolExecutionContext) -> ToolExecResult;

    /// Absolute writable roots this provider enforces for its write tools
    /// (F-12 safety evidence), for the tool call's executing agent context
    /// (F-19 role-aware policies). Providers that cannot project an
    /// enforceable boundary return `None`, and approval-time safety
    /// assessment falls through to the normal user flow.
    fn writable_roots_for(
        &self,
        _context: &ToolExecutionContext,
    ) -> Option<Vec<std::path::PathBuf>> {
        None
    }
}
