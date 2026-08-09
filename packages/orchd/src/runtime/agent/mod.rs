mod actor;
mod mailbox;
mod scope;

pub use scope::SessionAgentScope;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures_util::FutureExt;
use piko_comms::contracts::{
    AgentCommandReply, AgentCommands, AgentSnapshot as AgentSnapshotContract,
};
use piko_orchd_api::{
    AgentApiError, AgentRecoveryState, AgentRuntimeApi, SessionAgentConfig, SessionAgentHandle,
};
use piko_protocol::{
    AgentActivity, AgentCancelReceipt, AgentDurableCommand, AgentInboxSnapshot, AgentInputReceipt,
    AgentInstanceIdentity, AgentInstanceLifecycle, AgentLifecycleReceipt, AgentLifecycleRequest,
    AgentSnapshot, CreateAgentReceipt, CreateAgentRequest, MailboxWaitRequest, MailboxWaitSummary,
    SendAgentInputRequest, SteerAgentRequest,
};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use self::actor::AgentActor;
use self::mailbox::{AgentCommand, AgentHandle};
use super::execution::AgentExecutionRuntime;
use crate::ports::model_gateway::InferenceGateway;
use crate::runtime::reliability::RunCancellation;

/// Mandatory facade and Actor supervisor for multi-agent runtime operations.
pub struct AgentRuntime {
    execution: Arc<AgentExecutionRuntime>,
    sessions: RwLock<HashMap<String, Arc<SessionAgentScope>>>,
    accepting: AtomicBool,
    context_tools: Arc<crate::adapters::tools::ContextToolsProvider>,
}

mod api_impl;
mod runtime_impl;

fn agent_depth(parents: &HashMap<String, Option<String>>, agent_instance_id: &str) -> usize {
    let mut depth = 0;
    let mut current = parents.get(agent_instance_id).and_then(Clone::clone);
    while let Some(parent) = current {
        depth += 1;
        if depth > parents.len() {
            break;
        }
        current = parents.get(&parent).and_then(Clone::clone);
    }
    depth
}
