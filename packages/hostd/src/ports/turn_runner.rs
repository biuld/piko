use std::pin::Pin;
use std::sync::Arc;

use crate::api::{ProtocolError, ServerMessage};
use async_trait::async_trait;
use futures_core::Stream;
use piko_orchd_api::SessionSubscription;

use super::{NoopTrajectoryRegistry, TrajectoryRegistryPort};

pub type TurnEventStream = Pin<Box<dyn Stream<Item = Result<ServerMessage, ProtocolError>> + Send>>;

/// Identity of one admitted AgentInput as a work handle. `input_id` is the
/// durable control identity (the root input id for a work root).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentWorkAddress {
    pub session_id: String,
    pub input_id: String,
    pub agent_instance_id: String,
}

#[derive(Clone)]
pub struct ResumeAgent {
    pub agent_instance_id: String,
    pub state: piko_protocol::agent_runtime::AgentResumeState,
}

#[derive(Debug)]
pub struct AgentRunCompletion {
    pub input_id: String,
    pub result: Result<piko_protocol::AgentWorkReport, AgentRunFailure>,
    pub observation_barrier: piko_protocol::agent_runtime::SessionCursor,
}

pub trait OperationRunCompletion: Send {
    fn input_id(&self) -> &str;
    fn observation_barrier(&self) -> &piko_protocol::agent_runtime::SessionCursor;
}

impl OperationRunCompletion for AgentRunCompletion {
    fn input_id(&self) -> &str {
        &self.input_id
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

    /// Canonical sole admission path for an AgentInput, with host-private
    /// runtime extras (prompt staging, tool restriction, Turn correlation).
    /// The durable facts stay on `input`; extras are never persisted.
    async fn submit_agent_input(
        &self,
        _input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, ProtocolError> {
        Err(ProtocolError::InvalidCommand(
            "Agent input admission is unavailable".into(),
        ))
    }

    /// Bootstrap (idempotently) the runtime session an AgentInstance lives in.
    /// Registration, attached agents, resume, and observation routing happen
    /// here; admission follows as a separate canonical call.
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        _session_dir: &std::path::Path,
        _resume_agent: Option<&ResumeAgent>,
    ) -> Result<(), ProtocolError> {
        Ok(())
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

    /// Subscribe to the live observation stream for one admitted input. This
    /// resolves once `input_id` is the active root, has produced a report, or
    /// is no longer a pending follow-up.
    async fn wait_agent_input_started(
        &self,
        _session_id: &str,
        _agent_instance_id: &str,
        _input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<SessionSubscription, ProtocolError> {
        Err(ProtocolError::InvalidCommand(
            "agent input observation is unavailable".into(),
        ))
    }

    /// Observe the durable terminal report for one root input. This is a
    /// latest-state query, not a second admission or work handle.
    async fn wait_agent_input_completion(
        &self,
        _session_id: &str,
        _agent_instance_id: &str,
        _input_id: &str,
    ) -> Result<AgentRunCompletion, ProtocolError> {
        Err(ProtocolError::InvalidCommand(
            "agent input completion is unavailable".into(),
        ))
    }

    /// Release observation/route state for one admitted input. Never cancels
    /// or redelivers work; it only unregisters the live route.
    async fn finish_agent_run(&self, _session_id: &str, _agent_instance_id: &str, _input_id: &str) {
    }

    async fn cancel_queued_agent_run(&self, _: &str, _: &str, _: &str) -> bool {
        false
    }

    async fn recover_observation(
        &self,
        _session_id: &str,
        _agent_instance_id: &str,
        _input_id: &str,
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

    /// Agent-addressed interrupt of whatever Execution is currently active.
    /// It intentionally does not require a host Turn.
    // (kept for the deprecated `cancel_agent_run(--)` no-op surface below)
    async fn cancel_agent_run(&self, _: &str, _: &str) -> bool {
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
    async fn submit_agent_input(
        &self,
        _input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, ProtocolError> {
        Err(ProtocolError::InvalidCommand(self.message.clone()))
    }
}
