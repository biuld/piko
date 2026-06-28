// ---- Ports: abstractions for external capabilities ----
//
// Ports define interfaces (traits) that the domain needs but does not implement.
// Concrete implementations live in adapters/.

pub mod agent_spawner;
pub mod approval_gateway;
pub mod model_gateway;
pub mod tool_provider;

pub use agent_spawner::AgentSpawner;
pub use approval_gateway::ApprovalGateway;
pub use tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};
