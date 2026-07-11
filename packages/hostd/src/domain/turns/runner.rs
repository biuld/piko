use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use orchd::SessionSubscription;
use std::pin::Pin;
use tokio::sync::mpsc::UnboundedSender;

use crate::api::{ProtocolError, ServerMessage};
use orchd::integration::PersistSink;

pub type TurnEventStream = Pin<Box<dyn Stream<Item = Result<ServerMessage, ProtocolError>> + Send>>;

#[derive(Clone)]
pub struct ResumeRootTask {
    pub task_id: String,
    pub history: Vec<piko_protocol::Message>,
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
    /// Session storage directory for durable persist barrier.
    pub session_dir: Option<PathBuf>,
    /// Optional in-process persist sink override.
    pub persist_sink: Option<Arc<dyn PersistSink>>,
    /// Optional channel for host-visible side events (approvals, interactions).
    pub event_tx: Option<UnboundedSender<ServerMessage>>,
    /// Reattach a resumed root task with committed transcript history.
    pub resume_root_task: Option<ResumeRootTask>,
}

#[async_trait]
pub trait TurnRunner: Send + Sync {
    /// Run a turn and return a session output subscription.
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, ProtocolError>;

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
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, ProtocolError> {
        use orchd::host::{SessionOutputHub, merged_output_stream};
        use piko_protocol::agent_runtime::{
            SessionEvent, SessionEventEnvelope, TaskSnapshot, TaskStatus,
        };
        use piko_protocol::{Message, MessageContent, MessageRole};

        let hub = Arc::new(SessionOutputHub::new(
            input.session_id.clone(),
            format!("mock_{}", uuid::Uuid::new_v4()),
            64,
        ));
        let cursor = hub.cursor();
        let subscription = merged_output_stream(hub.subscribe(), cursor.clone());
        let session_id = input.session_id.clone();
        let work_id = input.work_id.clone();
        let task_id = input.work_id.clone();
        let prompt = input.prompt.clone();
        let mut committed_user: Option<String> = None;

        if let Some(path) = input.session_dir.as_ref() {
            use crate::infra::storage::TaskShardHeader;
            use crate::infra::storage::task_repository::SESSION_SCHEMA_VERSION;
            let repository = crate::infra::storage::TaskRepository::new(path);
            let header = TaskShardHeader {
                schema_version: SESSION_SCHEMA_VERSION,
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "main".into(),
                parent_task_id: None,
                created_at: crate::protocol::now_ms(),
            };
            let _ = repository.create_task(header);
            let message_id = format!("msg_{}", uuid::Uuid::new_v4());
            let _ = repository.commit_message(orchd::integration::MessageCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "main".into(),
                work_id: work_id.clone(),
                task_seq: 1,
                message_id: message_id.clone(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String(prompt),
                    timestamp: Some(crate::protocol::now_ms()),
                },
                committed_at: crate::protocol::now_ms(),
            });
            committed_user = Some(message_id);
        }

        let hub_task = Arc::clone(&hub);
        let committed_user = committed_user;

        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let publish = async |task_id: String, event: SessionEvent| {
                let _ = hub_task
                    .publish_event(SessionEventEnvelope {
                        task_id,
                        agent_id: "main".into(),
                        task_seq: 1,
                        cursor: hub_task.cursor(),
                        event,
                    })
                    .await;
            };

            publish(
                task_id.clone(),
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id: session_id.clone(),
                        task_id: task_id.clone(),
                        agent_id: "main".into(),
                        parent_task_id: None,
                        status: TaskStatus::Created,
                        active_work: None,
                    },
                },
            )
            .await;

            if let Some(message_id) = committed_user {
                publish(
                    task_id.clone(),
                    SessionEvent::MessageCommitted {
                        message_id,
                        work_id: work_id.clone(),
                        role: MessageRole::User,
                    },
                )
                .await;
            }

            publish(
                task_id.clone(),
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id,
                        task_id,
                        agent_id: "main".into(),
                        parent_task_id: None,
                        status: TaskStatus::Idle,
                        active_work: None,
                    },
                },
            )
            .await;
        });
        std::mem::forget(hub);

        Ok(SessionSubscription {
            session_id: input.session_id,
            cursor,
            output: subscription,
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
    async fn run_turn_subscription(
        &self,
        _input: TurnRunInput,
    ) -> Result<SessionSubscription, ProtocolError> {
        Err(ProtocolError::InvalidCommand(self.message.clone()))
    }
}
