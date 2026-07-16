use std::path::PathBuf;
use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use orchd_api::SessionSubscription;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use crate::api::{ProtocolError, ServerMessage};

pub type TurnEventStream = Pin<Box<dyn Stream<Item = Result<ServerMessage, ProtocolError>> + Send>>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentOperationAddress {
    pub session_id: String,
    pub operation_id: String,
    pub agent_instance_id: String,
}

#[derive(Clone)]
pub struct ResumeAgent {
    pub agent_instance_id: String,
    pub state: piko_protocol::agent_runtime::AgentResumeState,
}

#[derive(Clone)]
pub struct AgentRunInput {
    pub session_id: String,
    pub operation_id: String,
    pub agent_instance_id: String,
    pub prompt: String,
    pub source_turn_id: Option<String>,
    pub prompt_resources: Option<piko_protocol::PromptResourceSnapshot>,
    pub cwd: String,
    /// Active tool names to enable. None = all tools enabled.
    pub active_tool_names: Option<Vec<String>>,
    /// Session storage directory for the durable AgentInstance shard.
    pub session_dir: PathBuf,
    /// Turn-scoped queue for approval/interaction UI prompts.
    /// The turn handler drains this and forwards to TUI on the main outbound channel.
    pub ui_event_tx: UnboundedSender<ServerMessage>,
    /// Reattach a resumed root agent with committed transcript history.
    pub resume_agent: Option<ResumeAgent>,
}

pub struct AgentRunHandle {
    pub address: AgentOperationAddress,
    pub observation: SessionSubscription,
    pub completion: AgentRunCompletionReceiver,
}

pub type AgentRunCompletionReceiver = oneshot::Receiver<AgentRunCompletion>;

#[derive(Debug)]
pub struct AgentRunCompletion {
    pub address: AgentOperationAddress,
    pub result: Result<piko_protocol::AgentRunReport, AgentRunFailure>,
    pub observation_barrier: piko_protocol::agent_runtime::SessionCursor,
}

pub trait OperationRunCompletion: Send {
    fn operation_address(&self) -> AgentOperationAddress;
    fn observation_barrier(&self) -> &piko_protocol::agent_runtime::SessionCursor;
}

impl OperationRunCompletion for AgentRunCompletion {
    fn operation_address(&self) -> AgentOperationAddress {
        self.address.clone()
    }

    fn observation_barrier(&self) -> &piko_protocol::agent_runtime::SessionCursor {
        &self.observation_barrier
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunFailure {
    pub message: String,
}

#[async_trait]
pub trait AgentRunRunner: Send + Sync {
    async fn run_agent(&self, _: AgentRunInput) -> Result<AgentRunHandle, ProtocolError> {
        Err(ProtocolError::InvalidCommand(
            "Agent run is unavailable".into(),
        ))
    }

    async fn finish_agent_run(
        &self,
        _: &AgentOperationAddress,
        _: &piko_protocol::agent_runtime::SessionCursor,
    ) {
    }

    async fn recover_observation(
        &self,
        _: &AgentOperationAddress,
    ) -> Result<
        (
            piko_protocol::agent_runtime::SessionRuntimeSnapshot,
            SessionSubscription,
        ),
        ProtocolError,
    > {
        Err(ProtocolError::InvalidCommand(
            "session subscription recovery is unavailable".into(),
        ))
    }

    async fn respond_approval(
        &self,
        _: &str,
        _: crate::api::ApprovalDecision,
    ) -> Result<bool, ProtocolError> {
        Ok(false)
    }

    async fn respond_user_interaction(
        &self,
        _: &str,
        _: crate::api::UserInteractionResponse,
    ) -> Result<bool, ProtocolError> {
        Ok(false)
    }

    async fn steer_agent(&self, _: &str, _: &str, _: &str) -> bool {
        false
    }

    async fn cancel_agent_run(&self, _: &AgentOperationAddress) -> bool {
        false
    }

    async fn has_active_session_run(&self, _: &str) -> bool {
        false
    }

    async fn list_agent_instances(&self, _: &str) -> Option<Vec<crate::api::AgentInfo>> {
        None
    }

    /// In-process pending approvals/interactions for recoverable session projection.
    async fn pending_prompts_for_session(
        &self,
        _: &str,
    ) -> (
        Vec<crate::api::ApprovalSnapshot>,
        Vec<crate::api::UserInteractionSnapshot>,
    ) {
        (Vec::new(), Vec::new())
    }
}

#[derive(Debug, Clone)]
pub struct ErrorAgentRunRunner {
    message: String,
}

impl ErrorAgentRunRunner {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[async_trait]
impl AgentRunRunner for ErrorAgentRunRunner {
    async fn run_agent(&self, _: AgentRunInput) -> Result<AgentRunHandle, ProtocolError> {
        Err(ProtocolError::InvalidCommand(self.message.clone()))
    }
}
