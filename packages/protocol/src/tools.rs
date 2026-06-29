// ---- Protocol: tools — tool definitions, tool sets, policies ----

use serde::{Deserialize, Serialize};

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
