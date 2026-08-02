// ---- Adapters: tools — tool registry and built-in providers ----

mod catalog;
pub mod multi_agent_provider;
pub mod registry;
pub mod todo_provider;
pub mod user_interaction_provider;
mod workspace_handlers;
pub mod workspace_provider;
pub use multi_agent_provider::MultiAgentToolProvider;
