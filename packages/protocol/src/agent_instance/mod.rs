//! Long-lived multi-agent identities and host/orchestrator DTOs.
//!
//! An `AgentInstance` is a stable Session member. It may own many short-lived
//! Executions, but Execution state is never folded into its lifecycle.

mod durable;
mod identity;
mod mailbox;
mod run;
#[cfg(test)]
mod tests;

pub use durable::*;
pub use identity::*;
pub use mailbox::*;
pub use run::*;
