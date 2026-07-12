use std::path::PathBuf;
use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use orchd_api::SessionSubscription;
use tokio::sync::mpsc::UnboundedSender;

use crate::api::{ProtocolError, ServerMessage};

pub type TurnEventStream = Pin<Box<dyn Stream<Item = Result<ServerMessage, ProtocolError>> + Send>>;

#[derive(Clone)]
pub struct ResumeRootAgent {
    pub agent_instance_id: String,
    pub state: piko_protocol::agent_runtime::AgentResumeState,
}

#[derive(Clone)]
pub struct TurnRunInput {
    pub session_id: String,
    pub turn_id: String,
    pub work_id: String,
    pub prompt: String,
    pub system_prompt: String,
    pub cwd: String,
    /// Active tool names to enable. None = all tools enabled.
    pub active_tool_names: Option<Vec<String>>,
    /// Session storage directory for the durable AgentInstance shard.
    pub session_dir: PathBuf,
    /// Turn-scoped queue for approval/interaction UI prompts.
    /// The turn handler drains this and forwards to TUI on the main outbound channel.
    pub ui_event_tx: UnboundedSender<ServerMessage>,
    /// Reattach a resumed root agent with committed transcript history.
    pub resume_root_agent: Option<ResumeRootAgent>,
}

#[async_trait]
pub trait TurnRunner: Send + Sync {
    /// Run a turn and return a session output subscription.
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, ProtocolError>;

    async fn recover_session_subscription(
        &self,
        _session_id: &str,
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
        _approval_id: &str,
        _decision: crate::api::ApprovalDecision,
    ) -> Result<bool, ProtocolError> {
        Ok(false)
    }

    async fn respond_user_interaction(
        &self,
        _interaction_id: &str,
        _response: crate::api::UserInteractionResponse,
    ) -> Result<bool, ProtocolError> {
        Ok(false)
    }

    /// Route a steering message to the active orchd task / execution.
    /// Returns true if the steering was delivered.
    async fn steer_task(
        &self,
        _session_id: &str,
        _task_id: &str,
        _source_task_id: &str,
        _source_agent_id: &str,
        _message: &str,
    ) -> bool {
        false
    }

    /// Cancel the active Execution for a session (Execution-runtime path).
    /// Returns true if a cancel was accepted.
    async fn cancel_execution(&self, _session_id: &str, _turn_id: &str) -> bool {
        false
    }

    async fn list_agent_instances(&self, _session_id: &str) -> Option<Vec<crate::api::AgentInfo>> {
        None
    }

    /// Register task identity for approval pre-checks and scoped grants.
    async fn on_task_created(&self, _task_id: &str, _session_id: &str, _cwd: &str) {}
}

#[derive(Debug, Clone)]
pub struct ErrorTurnRunner {
    message: String,
}

impl ErrorTurnRunner {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[async_trait]
impl TurnRunner for ErrorTurnRunner {
    async fn run_turn_subscription(
        &self,
        _input: TurnRunInput,
    ) -> Result<SessionSubscription, ProtocolError> {
        Err(ProtocolError::InvalidCommand(self.message.clone()))
    }
}
