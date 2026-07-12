use async_trait::async_trait;
use piko_protocol::agent_runtime::WorkSnapshot;
use piko_protocol::{Message, TaskEvent};

#[derive(Debug, Clone, PartialEq)]
pub struct MessageCommit {
    pub session_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub agent_instance_id: Option<String>,
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

/// Legacy Task lifecycle append (storage read/repair only; not on PersistSink).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskEventCommit {
    pub session_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub task_seq: u64,
    pub event: TaskEvent,
    pub committed_at: i64,
}

/// Legacy Work lifecycle append (storage read/repair only; not on PersistSink).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkEventCommit {
    pub session_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub task_seq: u64,
    pub snapshot: WorkSnapshot,
    pub committed_at: i64,
}

/// Header-only task/execution shard creation (no lifecycle records).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskShardEnsure {
    pub session_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub agent_instance_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub created_at: i64,
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

/// Durable write surface for Execution: messages + header-only shard ensure.
///
/// Legacy Task/Work lifecycle writers live on `TaskRepository` only (read/repair).
#[async_trait]
pub trait PersistSink: Send + Sync {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError>;

    /// Ensure a durable shard exists without appending Task/Work lifecycle records.
    async fn ensure_task_shard(&self, ensure: TaskShardEnsure) -> Result<PersistAck, PersistError> {
        let _ = ensure;
        Err(PersistError::Failed(
            "ensure_task_shard not implemented; override with header-only shard create".into(),
        ))
    }
}
