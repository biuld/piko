use async_trait::async_trait;
use futures_core::Stream;
use orchd::runtime::dispatch::SessionChannels;
use std::pin::Pin;

use crate::api::{ProtocolError, ServerMessage};

pub type TurnEventStream = Pin<Box<dyn Stream<Item = Result<ServerMessage, ProtocolError>> + Send>>;

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

#[async_trait]
pub trait TurnRunner: Send + Sync {
    /// Run a turn, returning a merged event stream.
    async fn run_turn(&self, input: TurnRunInput) -> Result<TurnEventStream, ProtocolError>;

    /// Run a turn with typed channel dispatch.
    /// Returns SessionChannels with separate persist / display streams.
    /// Default implementation wraps run_turn's stream into channels.
    async fn run_turn_channels(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionChannels, ProtocolError> {
        let channels = SessionChannels::new(Default::default());
        let stream = self.run_turn(input).await?;
        let persist_tx = channels.persist_sender();
        let display_tx = channels.display_sender();

        use orchd::runtime::dispatch::{
            display_events_from_server_message, persist_events_from_server_message,
        };
        use tokio_stream::StreamExt;

        tokio::spawn(async move {
            let mut stream = stream;
            while let Some(item) = stream.next().await {
                if let Ok(event) = item {
                    for display in display_events_from_server_message(&event) {
                        let _ = display_tx.send(display).await;
                    }
                    for persist in persist_events_from_server_message(&event) {
                        let _ = persist_tx.send(persist).await;
                    }
                }
            }
        });

        Ok(channels)
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
    async fn run_turn(&self, input: TurnRunInput) -> Result<TurnEventStream, ProtocolError> {
        let _ = input;
        Ok(Box::pin(tokio_stream::empty()))
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
    async fn run_turn(&self, input: TurnRunInput) -> Result<TurnEventStream, ProtocolError> {
        let _ = input;
        Err(ProtocolError::InvalidCommand(self.message.clone()))
    }
}
