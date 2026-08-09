// ---- Adapters: tools — tool registry and built-in providers ----

mod catalog;
pub mod context_tools_provider;
mod exec_handlers;
#[cfg(test)]
mod exec_handlers_tests;
mod features;
pub mod multi_agent_provider;
pub mod registry;
pub mod todo_provider;
pub mod user_interaction_provider;
mod workspace_handlers;
pub mod workspace_provider;
pub use context_tools_provider::{
    ContextToolsCallbacks, ContextToolsProvider, NewContextWindowCallback,
};
pub use multi_agent_provider::MultiAgentToolProvider;
pub(crate) use workspace_handlers::FILE_CHANGE_DETAILS_KEY;

#[cfg(test)]
mod registry_retry_tests;
#[cfg(test)]
mod registry_tests;
