use async_trait::async_trait;

use crate::api::{Event, ProtocolError};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
pub struct TurnRunInput {
    pub session_id: String,
    pub turn_id: String,
    pub prompt: String,
    pub system_prompt: String,
    pub cwd: String,
    /// Active tool names to enable. None = all tools enabled.
    pub active_tool_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct TurnRunOutput {
    pub events: Vec<Event>,
    pub total_tasks: u32,
}

#[async_trait]
pub trait TurnRunner: Send + Sync {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, ProtocolError>;

    async fn respond_approval(
        &self,
        _approval_id: &str,
        _decision: crate::api::ApprovalDecision,
    ) -> Result<bool, ProtocolError> {
        Ok(false)
    }

    /// Route a steering message to the active orchd task.
    /// Returns true if the steering was delivered.
    async fn steer_task(
        &self,
        _task_id: &str,
        _source_task_id: &str,
        _source_agent_id: &str,
        _message: &str,
    ) -> bool {
        false
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockTurnRunner;

#[async_trait]
impl TurnRunner for MockTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, ProtocolError> {
        let _ = input;
        Ok(TurnRunOutput {
            events: Vec::new(),
            total_tasks: 1,
        })
    }
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
    async fn run_turn(
        &self,
        input: TurnRunInput,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, ProtocolError> {
        let _ = input;
        Err(ProtocolError::InvalidCommand(self.message.clone()))
    }
}
