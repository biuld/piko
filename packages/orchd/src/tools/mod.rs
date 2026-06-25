// ---- Tools module — tool registry, orchestration control, workspace, user interaction ----

pub mod registry;
pub mod rpc_host_provider;
pub mod task_control_provider;
pub mod user_interaction_provider;
pub mod workspace_provider;

pub use registry::{CatalogRoute, ToolRegistry, ToolRegistryImpl};
pub use rpc_host_provider::RpcHostToolProvider;
pub use task_control_provider::TaskControlProvider;
pub use user_interaction_provider::{UserInteractionCallbacks, UserInteractionProvider};
pub use workspace_provider::WorkspaceToolProvider;
