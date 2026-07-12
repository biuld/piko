//! Adapt legacy PersistSink into ExecutionCommitPort.
//!
//! Compatibility bridge: `execution_id` is written as `task_id` and `turn_id`
//! as `work_id` until Execution-native storage replaces task shards.
//!
//! Phase 6: Execution path no longer appends Task/Work lifecycle records —
//! only shard header + message commits.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use orchd_api::{
    ExecutionCommitPort, MessageCommit as LegacyMessageCommit, PersistSink, TaskShardEnsure,
};
use piko_protocol::execution::{
    CommitAck, CommitError, ExecutionOutcomeCommit, MessageCommit,
};
use tokio::sync::Mutex;

pub struct LegacyPersistExecutionCommitPort {
    sink: Arc<dyn PersistSink>,
    agent_id: String,
    next_seq: Mutex<HashMap<String, u64>>,
}

impl LegacyPersistExecutionCommitPort {
    pub fn new(sink: Arc<dyn PersistSink>, agent_id: impl Into<String>) -> Self {
        Self {
            sink,
            agent_id: agent_id.into(),
            next_seq: Mutex::new(HashMap::new()),
        }
    }

    async fn next_seq(&self, execution_id: &str) -> u64 {
        let mut map = self.next_seq.lock().await;
        let entry = map.entry(execution_id.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Ensure a durable shard exists for this execution (header only).
    pub async fn ensure_execution_shard(
        &self,
        session_id: &str,
        execution_id: &str,
        _turn_id: &str,
    ) -> Result<(), CommitError> {
        self.sink
            .ensure_task_shard(TaskShardEnsure {
                session_id: session_id.to_string(),
                task_id: execution_id.to_string(),
                agent_id: self.agent_id.clone(),
                parent_task_id: None,
                created_at: chrono::Utc::now().timestamp_millis(),
            })
            .await
            .map_err(map_persist_error)?;
        // First message will use task_seq = 1.
        let mut map = self.next_seq.lock().await;
        map.entry(execution_id.to_string()).or_insert(0);
        Ok(())
    }
}

#[async_trait]
impl ExecutionCommitPort for LegacyPersistExecutionCommitPort {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError> {
        let task_seq = self.next_seq(&commit.execution_id).await;
        self.sink
            .commit_message(LegacyMessageCommit {
                session_id: commit.session_id.clone(),
                task_id: commit.execution_id.clone(),
                agent_id: self.agent_id.clone(),
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
        let revision = {
            let map = self.next_seq.lock().await;
            map.get(&commit.execution_id).copied().unwrap_or(0)
        };
        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
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
