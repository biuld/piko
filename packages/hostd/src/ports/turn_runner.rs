use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crate::api::{ProtocolError, ServerMessage};
use async_trait::async_trait;
use futures_core::Stream;
use piko_orchd_api::SessionSubscription;

use super::{NoopTrajectoryRegistry, TrajectoryRegistryPort};

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
    pub content: piko_protocol::MessageContent,
    pub source_turn_id: Option<String>,
    pub prompt_resources: Option<piko_protocol::PromptResourceSnapshot>,
    pub cwd: String,
    /// Active tool names to enable. None = all tools enabled.
    pub active_tool_names: Option<Vec<String>>,
    /// Session storage directory for the durable journal.
    pub session_dir: PathBuf,
    /// Reattach a resumed root agent with committed transcript history.
    pub resume_agent: Option<ResumeAgent>,
}

impl AgentRunInput {
    pub fn text_projection(&self) -> String {
        match &self.content {
            piko_protocol::MessageContent::String(text) => text.clone(),
            piko_protocol::MessageContent::Blocks(blocks) => blocks
                .iter()
                .map(piko_protocol::ContentBlock::text_projection)
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

pub struct AgentRunHandle {
    pub address: AgentOperationAddress,
    pub receipt: piko_protocol::AgentInputReceipt,
    pub process: Box<dyn AgentRunProcess>,
}

#[async_trait]
pub trait AgentRunProcess: Send {
    async fn wait_started(&mut self) -> Result<SessionSubscription, ProtocolError>;

    async fn wait_completion(self: Box<Self>) -> Result<AgentRunCompletion, ProtocolError>;
}

#[derive(Debug)]
pub struct AgentRunCompletion {
    pub address: AgentOperationAddress,
    pub result: Result<piko_protocol::AgentWorkReport, AgentRunFailure>,
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
    /// Shared trajectory recorder registry (F-36). Defaults to a no-op for
    /// runners without trajectory capture.
    fn trajectory_registry(&self) -> Arc<dyn TrajectoryRegistryPort> {
        Arc::new(NoopTrajectoryRegistry)
    }

    /// Live set of external processes spawned by the `process` tool (F-08);
    /// empty when the runner has no process manager.
    async fn list_processes(&self) -> Vec<piko_protocol::command::ProcessInfo> {
        Vec::new()
    }

    /// Terminate one external process; `None` when it is not running.
    async fn terminate_process(
        &self,
        _process_id: &str,
    ) -> Option<piko_protocol::command::ProcessExit> {
        None
    }

    /// MCP server connection status (F-13); empty when the runner has no
    /// MCP configuration.
    async fn mcp_statuses(&self) -> Vec<piko_protocol::command::McpServerInfo> {
        Vec::new()
    }

    /// Seed orchd runtime todo store from host durable lists (F-27 hydrate).
    async fn seed_todo_lists(&self, _lists: Vec<piko_protocol::TodoList>) {}

    async fn run_agent(&self, _: AgentRunInput) -> Result<AgentRunHandle, ProtocolError> {
        Err(ProtocolError::InvalidCommand(
            "Agent run is unavailable".into(),
        ))
    }

    /// Canonical admission path for AgentInputs that need no host-private
    /// prompt staging (steers and agent-to-agent inputs).
    async fn submit_agent_input(
        &self,
        _input: piko_protocol::AgentInput,
    ) -> Result<piko_protocol::AgentInputReceipt, ProtocolError> {
        Err(ProtocolError::InvalidCommand(
            "Agent input admission is unavailable".into(),
        ))
    }

    async fn cancel_agent_input(
        &self,
        _session_id: &str,
        _agent_instance_id: &str,
        _input_id: &str,
    ) -> Result<piko_protocol::AgentInputCancelReceipt, ProtocolError> {
        Err(ProtocolError::InvalidCommand(
            "Agent input cancellation is unavailable".into(),
        ))
    }

    async fn finish_agent_run(
        &self,
        _: &AgentOperationAddress,
        _: &piko_protocol::agent_runtime::SessionCursor,
    ) {
    }

    async fn cancel_queued_agent_run(&self, _: &AgentOperationAddress) -> bool {
        false
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

    async fn cancel_agent_run(&self, _: &AgentOperationAddress) -> bool {
        false
    }

    /// Interrupt whichever Execution is currently active for an AgentInstance.
    /// This is agent-addressed and intentionally does not require a host Turn.
    async fn interrupt_agent(&self, _: &str, _: &str) -> bool {
        false
    }

    async fn has_active_session_run(&self, _: &str) -> bool {
        false
    }

    async fn list_agent_instances(&self, _: &str) -> Option<Vec<crate::api::AgentInfo>> {
        None
    }

    /// Wire the `new_context_window` tool callback (F-05). Default no-op;
    /// the orchd runner forwards to its context-tools provider.
    fn set_context_window_callback(&self, _: piko_orchd::tools::NewContextWindowCallback) {}

    /// Wire the F-11 guardian review callback. Default no-op; the orchd
    /// runner forwards to its approval gateway.
    fn set_guardian_review_callback(&self, _: crate::domain::guardian::GuardianReviewCallback) {}

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
