// ---- Adapters: tools — tool registry and built-in providers ----

pub mod registry;
pub mod task_control_provider;
pub mod todo_provider;
pub mod user_interaction_provider;
pub mod workspace_provider;

pub use registry::{CatalogRoute, ToolRegistry, ToolRegistryImpl};
pub use task_control_provider::TaskControlProvider;
pub use todo_provider::TodoProvider;
pub use user_interaction_provider::{UserInteractionCallbacks, UserInteractionProvider};
pub use workspace_provider::WorkspaceToolProvider;
