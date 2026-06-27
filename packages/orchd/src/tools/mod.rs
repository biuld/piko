// ---- Tools module — tool registry, orchestration control, workspace, user interaction ----

pub mod api;
pub mod registry;
pub mod task_control_provider;
pub mod user_interaction_provider;
pub mod workspace_provider;

pub use api::{
    ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest, ToolDiscoveryContext,
    ToolExecError, ToolExecResult, ToolExecutionContext, ToolProvider, is_approval_accepted,
};
pub use registry::{CatalogRoute, ToolRegistry, ToolRegistryImpl};
pub use task_control_provider::TaskControlProvider;
pub use user_interaction_provider::{UserInteractionCallbacks, UserInteractionProvider};
pub use workspace_provider::WorkspaceToolProvider;
