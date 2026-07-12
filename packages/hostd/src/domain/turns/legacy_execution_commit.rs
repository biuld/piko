//! Adapt legacy PersistSink into ExecutionCommitPort.
//!
//! Single-agent storage: one stable root shard under `tasks/{root_id}.jsonl`
//! holds the conversation Message chain across Turns. Each Turn still has a
//! distinct runtime `execution_id`; this adapter maps commits onto the root
//! shard (`storage_task_id`) and continues `task_seq` from the prior head.
//!
//! Phase 6: no Task/Work lifecycle records — header (once) + messages only.

use std::sync::Arc;

use async_trait::async_trait;
use orchd_api::{
    ExecutionCommitPort, MessageCommit as LegacyMessageCommit, PersistSink, TaskShardEnsure,
};
use piko_protocol::execution::{CommitAck, CommitError, ExecutionOutcomeCommit, MessageCommit};
use tokio::sync::Mutex;

pub struct LegacyPersistExecutionCommitPort {
    sink: Arc<dyn PersistSink>,
    agent_id: String,
    /// Stable root transcript shard id (reused across Turns).
    storage_task_id: String,
    next_seq: Mutex<u64>,
}

impl LegacyPersistExecutionCommitPort {
    /// Bind commits to a root shard, continuing sequence after `last_task_seq`.
    pub fn for_root_shard(
        sink: Arc<dyn PersistSink>,
        agent_id: impl Into<String>,
        storage_task_id: impl Into<String>,
        last_task_seq: u64,
    ) -> Self {
        Self {
            sink,
            agent_id: agent_id.into(),
            storage_task_id: storage_task_id.into(),
            next_seq: Mutex::new(last_task_seq),
        }
    }

    pub fn storage_task_id(&self) -> &str {
        &self.storage_task_id
    }

    async fn next_seq(&self) -> u64 {
        let mut seq = self.next_seq.lock().await;
        *seq += 1;
        *seq
    }

    /// Create the root shard header when this is the first Turn (no prior resume).
    pub async fn ensure_root_shard_if_needed(
        &self,
        session_id: &str,
        create: bool,
    ) -> Result<(), CommitError> {
        if !create {
            return Ok(());
        }
        self.sink
            .ensure_task_shard(TaskShardEnsure {
                session_id: session_id.to_string(),
                task_id: self.storage_task_id.clone(),
                agent_id: self.agent_id.clone(),
                agent_instance_id: Some(format!("agent_{session_id}_root")),
                parent_task_id: None,
                created_at: chrono::Utc::now().timestamp_millis(),
            })
            .await
            .map_err(map_persist_error)?;
        Ok(())
    }
}

#[async_trait]
impl ExecutionCommitPort for LegacyPersistExecutionCommitPort {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError> {
        let task_seq = self.next_seq().await;
        self.sink
            .commit_message(LegacyMessageCommit {
                session_id: commit.session_id.clone(),
                task_id: self.storage_task_id.clone(),
                agent_id: self.agent_id.clone(),
                agent_instance_id: Some(commit.agent_instance_id.clone()),
                work_id: commit.turn_id.clone(),
                task_seq,
                message_id: commit.message_id.clone(),
                parent_message_id: commit.parent_message_id.clone(),
                message: commit.message.clone(),
                committed_at: commit.committed_at,
            })
            .await
            .map_err(map_persist_error)?;
        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision: task_seq,
        })
    }

    async fn commit_execution_outcome(
        &self,
        commit: ExecutionOutcomeCommit,
    ) -> Result<CommitAck, CommitError> {
        // Do not append Task/Work lifecycle records. Turn terminal is driven by
        // ExecutionChanged observation; message history remains authoritative.
        let revision = *self.next_seq.lock().await;
        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: None,
            revision,
        })
    }
}

fn map_persist_error(err: orchd_api::PersistError) -> CommitError {
    match err {
        orchd_api::PersistError::Unavailable => CommitError::Unavailable,
        orchd_api::PersistError::IdentityMismatch => CommitError::IdentityMismatch,
        orchd_api::PersistError::SequenceMismatch { expected, actual } => {
            CommitError::SequenceMismatch { expected, actual }
        }
        orchd_api::PersistError::IdempotencyConflict => CommitError::IdempotencyConflict,
        orchd_api::PersistError::Failed(msg) => CommitError::Failed(msg),
    }
}
