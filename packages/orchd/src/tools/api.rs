use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::protocol::agents::HostTaskContext;
use crate::protocol::messages::ToolCall;
use crate::protocol::tools::{ToolDef, ToolProviderSource};

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
}

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

pub trait ToolProvider: Send + Sync + 'static {
    fn id(&self) -> &str;

    fn source(&self) -> ToolProviderSource;

    fn discover(
        &self,
        context: ToolDiscoveryContext,
    ) -> Pin<Box<dyn Future<Output = Vec<ToolDef>> + Send + '_>>;

    fn execute(
        &self,
        call: ToolCall,
        context: ToolExecutionContext,
    ) -> Pin<Box<dyn Future<Output = ToolExecResult> + Send + '_>>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalRequest {
    #[serde(rename = "toolEntityId")]
    pub tool_entity_id: String,
    #[serde(rename = "callId")]
    pub call_id: String,
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(rename = "toolArgs")]
    pub tool_args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolApprovalDecision {
    Accept,
    Decline,
    AcceptSession,
    AcceptWorkspace,
    AcceptPermanent,
}

pub fn is_approval_accepted(decision: &ToolApprovalDecision) -> bool {
    !matches!(decision, ToolApprovalDecision::Decline)
}

pub trait ApprovalGateway: Send + Sync + 'static {
    fn request_tool_approval(
        &self,
        request: ToolApprovalRequest,
    ) -> Pin<Box<dyn Future<Output = ToolApprovalDecision> + Send + '_>>;
}
