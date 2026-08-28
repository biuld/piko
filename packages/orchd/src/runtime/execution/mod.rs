//! Short-lived Execution Actor for the single-agent path.

mod actor;
mod bootstrap;
mod budget;
mod mailbox;
mod prompt;
mod scope;
mod services;
pub(crate) mod state;
mod tool_batch;

pub use actor::ExecutionActor;
pub use mailbox::{ExecutionCommand, ExecutionHandle};
pub use scope::{ExecutionExit, SessionExecutionScope};
pub use services::ExecutionServices;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::FutureExt;
use piko_comms::contracts::{
    ExecutionCommandReply, ExecutionCommands, ExecutionTerminal as ExecutionTerminalContract,
};
use piko_orchd_api::{AgentApiError, CancelReceipt, SessionExecutionPorts};
use piko_protocol::execution::{
    CancelExecutionRequest, ExecutionInputReceipt, ExecutionReceipt, ExecutionStatus,
    StartExecutionRequest, SteerExecutionRequest,
};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry};
use crate::ports::model_gateway::InferenceGateway;
use crate::ports::tool_provider::ToolDiscoveryContext;
use crate::runtime::reliability::TerminalSelector;
use piko_protocol::agents::AgentSpec;
use piko_sandbox::exec::process::ProcessManager;

pub(crate) struct PreparedRunContext {
    pub prompt: piko_protocol::SemanticRunPrompt,
    pub tool_catalog: piko_protocol::ResolvedToolCatalog,
    pub routes: HashMap<String, CatalogRoute>,
}

/// AgentRuntime-internal Execution Actor supervisor.
pub struct AgentExecutionRuntime {
    services: ExecutionServices,
    /// Long-lived `process` tool manager, shared with the workspace
    /// provider and exposed for the hostd `/ps` surface (F-08).
    processes: Arc<ProcessManager>,
    sessions: RwLock<HashMap<String, Arc<SessionExecutionScope>>>,
    accepting: AtomicBool,
}

mod impls;
mod prepared;
mod setup;
mod terminal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionIdentity {
    pub session_id: String,
    /// Interaction Turn this Execution is bound to. `None` for child agent
    /// Executions spawned by multi-agent tools.
    pub source_turn_id: Option<String>,
    /// Logical agent attempt. Multiple concrete executions must not be
    /// conflated with this identity.
    pub run_id: String,
    pub execution_id: String,
    pub agent_instance_id: String,
    pub agent_id: String,
    /// F-19: role of the executing agent from the registered `AgentSpec`.
    /// `None` when the spec is not registered (inherits session policy).
    pub agent_role: Option<String>,
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        let mut result = text.chars().take(max).collect::<String>();
        result.push_str("...");
        result
    }
}

#[cfg(test)]
mod tests;
pub(crate) use prepared::PreparedExecution;
pub(crate) use terminal::ExecutionTerminal;
