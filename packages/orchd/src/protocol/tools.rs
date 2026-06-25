// ---- Protocol: tools — tool provider, tool set, policies ----

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use super::messages::ToolCall;

// ---- Enums / string types ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolCapability {
    #[serde(rename = "workspace_read")]
    WorkspaceRead,
    #[serde(rename = "workspace_write")]
    WorkspaceWrite,
    #[serde(rename = "process")]
    Process,
    Network,
    #[serde(rename = "view_image")]
    ViewImage,
    #[serde(rename = "update_plan")]
    UpdatePlan,
    #[serde(rename = "user_input")]
    UserInput,
    Delegation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolExposure {
    Direct,
    Deferred,
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolApprovalRequirement {
    Never,
    #[serde(rename = "on_request")]
    OnRequest,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolProviderSource {
    #[serde(rename = "orch")]
    Orch,
    Host,
    #[serde(rename = "workspace")]
    Workspace,
    #[serde(rename = "mcp")]
    Mcp,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolExecutionMode {
    Parallel,
    Sequential,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolSensitivity {
    Safe,
    Sensitive,
    Dangerous,
    Dynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolApprovalPolicy {
    Never,
    #[serde(rename = "on_sensitive")]
    OnSensitive,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolFailureMode {
    #[serde(rename = "return_error")]
    ReturnError,
    #[serde(rename = "fail_task")]
    FailTask,
}

// ---- ToolDef ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutorRef {
    pub kind: String, // native, host, remote, sandbox, mcp, orchestrator
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "readOnly")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "mutatesWorkspace")]
    pub mutates_workspace: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "producesArtifact")]
    pub produces_artifact: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    pub executor: ToolExecutorRef,
    #[serde(skip_serializing_if = "Option::is_none", rename = "executionMode")]
    pub execution_mode: Option<ToolExecutionMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure: Option<ToolExposure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<ToolCapability>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<ToolApprovalRequirement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ToolMetadata>,
}

// ---- ToolSets ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolSetMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>, // builtin, host, mcp, plugin, dynamic, agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<ToolSensitivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<ToolApprovalPolicy>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "timeoutMs")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "executionMode")]
    pub execution_mode: Option<ToolExecutionMode>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "failureMode")]
    pub failure_mode: Option<ToolFailureMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolSetPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<ToolPolicy>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "allowParallel")]
    pub allow_parallel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxConcurrentCalls")]
    pub max_concurrent_calls: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ToolSetToolRef {
    #[serde(rename = "provider_tool")]
    ProviderTool {
        #[serde(rename = "providerId")]
        provider_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        alias: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        policy: Option<ToolPolicy>,
    },
    #[serde(rename = "provider_namespace")]
    ProviderNamespace {
        #[serde(rename = "providerId")]
        provider_id: String,
        namespace: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        alias: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        policy: Option<ToolPolicy>,
    },
    #[serde(rename = "orchestrator_control")]
    OrchestratorControl {
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        alias: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        policy: Option<ToolPolicy>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolSet {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub tools: Vec<ToolSetToolRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<ToolSetPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ToolSetMetadata>,
}

// ---- ToolDiscoveryContext ----

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

// ---- ToolExecutionContext ----

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
}

// ---- ToolExecResult ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ToolExecError>,
}

// ---- ToolProvider trait ----

/// A ToolProvider is the discovery and execution adapter for one source of tools.
///
/// In Rust 2024, `async fn` in traits is stable — no #[async_trait] needed.
/// A ToolProvider is the discovery and execution adapter for one source of tools.
///
/// Uses `Pin<Box<dyn Future>>` return types for dyn compatibility.
pub trait ToolProvider: Send + Sync + 'static {
    /// Unique provider identifier.
    fn id(&self) -> &str;

    /// Source classification.
    fn source(&self) -> ToolProviderSource;

    /// Discover available tools for the given context.
    fn discover(
        &self,
        context: ToolDiscoveryContext,
    ) -> Pin<Box<dyn Future<Output = Vec<ToolDef>> + Send + '_>>;

    /// Execute a tool call.
    fn execute(
        &self,
        call: ToolCall,
        context: ToolExecutionContext,
    ) -> Pin<Box<dyn Future<Output = ToolExecResult> + Send + '_>>;
}
