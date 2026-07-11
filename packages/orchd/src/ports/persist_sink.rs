use async_trait::async_trait;
use piko_protocol::{Message, TaskEvent};

#[derive(Debug, Clone, PartialEq)]
pub struct MessageCommit {
    pub session_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub work_id: String,
    pub task_seq: u64,
    pub message_id: String,
    pub parent_message_id: Option<String>,
    pub message: Message,
    pub committed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistAck {
    pub session_id: String,
    pub task_id: String,
    pub message_id: Option<String>,
    pub task_seq: u64,
}

#[derive(Debug, Clone)]
pub struct TaskEventCommit {
    pub session_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub task_seq: u64,
    pub event: TaskEvent,
    pub committed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PersistError {
    #[error("persistence is unavailable")]
    Unavailable,
    #[error("persistence identity mismatch")]
    IdentityMismatch,
    #[error("persistence sequence mismatch: expected {expected}, got {actual}")]
    SequenceMismatch { expected: u64, actual: u64 },
    #[error("persistence idempotency conflict")]
    IdempotencyConflict,
    #[error("persistence failed: {0}")]
    Failed(String),
}

#[async_trait]
pub trait PersistSink: Send + Sync {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError>;

    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError>;
}
