//! Host integration surface for bootstrapping orchd inside hostd.
//!
//! Production turn execution should go through [`crate::AgentRuntime`] /
//! [`crate::AgentRuntimeService`]. This module exposes the wiring types hostd
//! needs to construct a supervisor, register tools, and bridge approvals.

pub use crate::adapters::tools::{
    UserInteractionCallbacks, UserInteractionProvider, UserInteractionRequest,
    registry::ToolRegistryImpl,
};
pub use crate::application::Supervisor;
pub use crate::domain::tools::{
    approval::{ToolApprovalDecision, ToolApprovalRequest},
    definition::{ToolSet, ToolSetToolRef},
    result::{ToolExecError, ToolExecResult},
};
pub use crate::domain::work::TaskReport;
pub use crate::ports::{
    approval_gateway::ApprovalGateway,
    tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider},
};
pub use crate::runtime::events::hub::{SessionOutputHub, merged_output_stream};
pub use crate::runtime::task::input::build_user_input;
