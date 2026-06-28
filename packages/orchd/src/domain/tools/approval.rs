// ---- Domain: tool approval ----

use serde::{Deserialize, Serialize};

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
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(rename = "toolArgs")]
    pub tool_args: serde_json::Value,
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
