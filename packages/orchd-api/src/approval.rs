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
    /// F-19: role of the executing agent, copied from the registered
    /// `AgentSpec`. Identity metadata only — hostd resolves the role's
    /// permission profile. Absent/unknown roles use the session profile.
    #[serde(rename = "agentRole", skip_serializing_if = "Option::is_none")]
    pub agent_role: Option<String>,
    /// F-13: provider id of the catalog route the tool call resolves to
    /// (for MCP tools this is the server name). Lets hostd resolve
    /// `server/tool` approval templates precisely. Absent for callers that
    /// do not carry route identity; template resolution falls back to bare
    /// `tool` keys.
    #[serde(rename = "providerId", skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(rename = "toolArgs")]
    pub tool_args: serde_json::Value,
    #[serde(rename = "hostContext", skip_serializing_if = "Option::is_none")]
    pub host_context: Option<HostSessionContext>,
    /// Absolute writable roots the requesting provider can enforce for this
    /// tool call (F-12 safety evidence). Absent when the provider cannot
    /// project them (e.g. non-workspace providers).
    #[serde(rename = "writableRoots", skip_serializing_if = "Option::is_none")]
    pub writable_roots: Option<Vec<String>>,
}

/// Decision on a tool approval request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ToolApprovalDecision {
    Accept,
    Decline,
    /// The approval request expired before a user decision arrived; the tool
    /// call fails closed with a distinct, non-retryable error.
    Expired,
    /// The guardian auto-review denied the request. The tool call fails
    /// closed with a distinct, non-retryable error that carries the reason.
    GuardianDenied {
        reason: String,
    },
    /// The guardian auto-review failed (timeout, malformed output, or model
    /// error). The tool call fails closed without running.
    GuardianUnavailable,
    /// The deterministic F-12 safety assessment rejected the request (e.g.
    /// a write target outside the sandbox writable roots). The tool call
    /// fails closed with a distinct, non-retryable error that carries the
    /// reason.
    SafetyRejected {
        reason: String,
    },
    /// The F-17 permission-profile command policy rejected the request
    /// (the operator denied the command prefix). The tool call fails closed
    /// with a distinct, non-retryable error that carries the reason.
    PermissionDenied {
        reason: String,
    },
    AcceptSession,
    AcceptWorkspace,
    AcceptPermanent,
}

/// Check whether an approval decision is accepting (not Decline).
pub fn is_approval_accepted(decision: &ToolApprovalDecision) -> bool {
    !matches!(
        decision,
        ToolApprovalDecision::Decline
            | ToolApprovalDecision::Expired
            | ToolApprovalDecision::GuardianDenied { .. }
            | ToolApprovalDecision::GuardianUnavailable
            | ToolApprovalDecision::SafetyRejected { .. }
            | ToolApprovalDecision::PermissionDenied { .. }
    )
}

/// Gateway for requesting tool execution approval from the integrator.
#[async_trait]
pub trait ApprovalGateway: Send + Sync + 'static {
    async fn request_tool_approval(&self, request: ToolApprovalRequest) -> ToolApprovalDecision;
}
