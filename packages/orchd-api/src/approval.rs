use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use piko_protocol::agents::HostSessionContext;

/// Request for tool execution approval.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalRequest {
    #[serde(rename = "toolEntityId")]
    pub tool_entity_id: String,
    #[serde(rename = "callId")]
    pub call_id: String,
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "agentInstanceId")]
    pub agent_instance_id: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(rename = "toolArgs")]
    pub tool_args: serde_json::Value,
    #[serde(rename = "hostContext", skip_serializing_if = "Option::is_none")]
    pub host_context: Option<HostSessionContext>,
}

/// Decision on a tool approval request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolApprovalDecision {
    Accept,
    Decline,
    AcceptSession,
    AcceptWorkspace,
    AcceptPermanent,
}

/// Check whether an approval decision is accepting (not Decline).
pub fn is_approval_accepted(decision: &ToolApprovalDecision) -> bool {
    !matches!(decision, ToolApprovalDecision::Decline)
}

/// Gateway for requesting tool execution approval from the integrator.
#[async_trait]
pub trait ApprovalGateway: Send + Sync + 'static {
    async fn request_tool_approval(&self, request: ToolApprovalRequest) -> ToolApprovalDecision;
}
