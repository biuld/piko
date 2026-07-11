use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use orchd::runtime::dispatch::SessionChannels;
use std::pin::Pin;

use crate::api::{ProtocolError, ServerMessage};
use orchd::integration::PersistSink;

pub type TurnEventStream = Pin<Box<dyn Stream<Item = Result<ServerMessage, ProtocolError>> + Send>>;

#[derive(Clone)]
pub struct TurnRunInput {
    pub session_id: String,
    pub turn_id: String,
    pub prompt: String,
    pub system_prompt: String,
    pub cwd: String,
    /// Active tool names to enable. None = all tools enabled.
    pub active_tool_names: Option<Vec<String>>,
    /// Session storage directory for durable persist barrier.
    pub session_dir: Option<PathBuf>,
    /// Optional in-process persist sink override.
    pub persist_sink: Option<Arc<dyn PersistSink>>,
}

#[async_trait]
pub trait TurnRunner: Send + Sync {
    /// Run a turn with typed channel dispatch.
    /// Returns SessionChannels with separate persist / display streams.
    async fn run_turn_channels(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionChannels, ProtocolError>;

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
    async fn run_turn_channels(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionChannels, ProtocolError> {
        let mut channels = SessionChannels::new(Default::default());
        channels.spawn_lifecycle_dispatch(input.session_id.clone());
        let senders = channels.senders();
        tokio::spawn(async move {
            let _ = senders.lifecycle.send(piko_protocol::LifecycleEvent::Task(
                piko_protocol::TaskEvent::Created {
                    session_id: input.session_id.clone(),
                    task_id: input.turn_id.clone(),
                    agent_id: "main".into(),
                    parent_task_id: None,
                    source_agent_id: None,
                    prompt: input.prompt.clone(),
                    turn_id: input.turn_id.clone(),
                    timestamp: crate::protocol::now_ms(),
                },
            ));
            let _ = senders
                .persist
                .send(std::sync::Arc::new(
                    piko_protocol::PersistEvent::UserCommitted {
                        session_id: input.session_id,
                        message_id: format!("msg_{}", uuid::Uuid::new_v4()),
                        task_id: input.turn_id.clone(),
                        agent_id: "main".into(),
                        work_id: input.turn_id,
                        message: piko_protocol::Message::User {
                            content: piko_protocol::MessageContent::String(input.prompt),
                            timestamp: Some(crate::protocol::now_ms()),
                        },
                    },
                ))
                .await;
        });
        Ok(channels)
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
    async fn run_turn_channels(
        &self,
        _input: TurnRunInput,
    ) -> Result<SessionChannels, ProtocolError> {
        Err(ProtocolError::InvalidCommand(self.message.clone()))
    }
}
