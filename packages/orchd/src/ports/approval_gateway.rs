// ---- Port: ApprovalGateway — interface for tool execution approval ----
//
// The approval gateway allows the host layer to gate tool execution.
// This is the port — concrete implementations live in the host/TUI bridge.

use async_trait::async_trait;

use crate::domain::tools::approval::{ToolApprovalDecision, ToolApprovalRequest};

/// Gateway for requesting tool execution approval from the host/user.
#[async_trait]
pub trait ApprovalGateway: Send + Sync + 'static {
    /// Request approval for a tool execution.
    /// Returns the host/user's decision.
    async fn request_tool_approval(
        &self,
        request: ToolApprovalRequest,
    ) -> ToolApprovalDecision;
}
