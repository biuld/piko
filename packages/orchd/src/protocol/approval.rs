// ---- Protocol: approval — approval gateway types ----

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

// ---- ToolApprovalRequest ----

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

// ---- ToolApprovalDecision ----

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

// ---- ApprovalGateway trait ----

/// ApprovalGateway is the interface for requesting tool approval from the user.
///
/// Uses `Pin<Box<dyn Future>>` return type for dyn compatibility.
pub trait ApprovalGateway: Send + Sync + 'static {
    fn request_tool_approval(
        &self,
        request: ToolApprovalRequest,
    ) -> Pin<Box<dyn Future<Output = ToolApprovalDecision> + Send + '_>>;
}
