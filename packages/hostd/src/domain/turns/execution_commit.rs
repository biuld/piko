//! Bridge ExecutionCommitPort → hostd HostState (+ legacy PersistSink).
//!
//! Temporary compatibility: durable writes reuse one root task shard.
//! Runtime `execution_id` remains distinct per Turn; storage maps commits onto
//! the stable root transcript id.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use orchd_api::{ExecutionCommitPort, MessageCommit as LegacyMessageCommit, PersistSink};
use piko_protocol::execution::{CommitAck, CommitError, ExecutionOutcomeCommit, MessageCommit};
use tokio::sync::Mutex;

use crate::domain::sessions::HostState;
use crate::domain::turns::session_output::append_committed_message;

/// Host-owned commit port for the single-agent Execution path.
pub struct HostExecutionCommitPort {
    state: Arc<Mutex<HostState>>,
    legacy: Option<Arc<dyn PersistSink>>,
    agent_id: String,
    next_seq: Mutex<HashMap<String, AtomicU64>>,
}

impl HostExecutionCommitPort {
    pub fn new(state: Arc<Mutex<HostState>>, agent_id: impl Into<String>) -> Self {
        Self {
            state,
            legacy: None,
            agent_id: agent_id.into(),
            next_seq: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_legacy_sink(mut self, sink: Arc<dyn PersistSink>) -> Self {
        self.legacy = Some(sink);
        self
    }

    async fn next_revision(&self, execution_id: &str) -> u64 {
        let mut map = self.next_seq.lock().await;
        let counter = map
            .entry(execution_id.to_string())
            .or_insert_with(|| AtomicU64::new(0));
        counter.fetch_add(1, Ordering::SeqCst) + 1
    }
}

#[async_trait]
impl ExecutionCommitPort for HostExecutionCommitPort {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError> {
        let revision = self.next_revision(&commit.execution_id).await;

        {
            let mut state = self.state.lock().await;
            append_committed_message(
                &mut state,
                &commit.session_id,
                &commit.execution_id,
                &self.agent_id,
                &commit.execution_id,
                &commit.message,
                &commit.message_id,
                revision,
                commit.parent_message_id.as_deref(),
            )
            .map_err(|err| CommitError::Failed(err.to_string()))?;
        }

        if let Some(legacy) = &self.legacy {
            let legacy_commit = LegacyMessageCommit {
                session_id: commit.session_id.clone(),
                task_id: commit.execution_id.clone(),
                agent_id: self.agent_id.clone(),
                agent_instance_id: Some(commit.agent_instance_id.clone()),
                work_id: commit.turn_id.clone(),
                task_seq: revision,
                message_id: commit.message_id.clone(),
                parent_message_id: commit.parent_message_id.clone(),
                message: commit.message.clone(),
                committed_at: commit.committed_at,
            };
            legacy
                .commit_message(legacy_commit)
                .await
                .map_err(map_persist_error)?;
        }

        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision,
        })
    }

    async fn commit_execution_outcome(
        &self,
        commit: ExecutionOutcomeCommit,
    ) -> Result<CommitAck, CommitError> {
        // Execution path does not append Task/Work lifecycle records.
        let revision = self.next_revision(&commit.execution_id).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sessions::SessionState;
    use piko_protocol::ContentBlock;

    #[tokio::test]
    async fn commits_assistant_into_host_state() {
        let state = Arc::new(Mutex::new(HostState::new()));
        {
            let mut guard = state.lock().await;
            guard.insert_session(SessionState::new("session-1".into(), "/tmp".into()));
        }

        let port = HostExecutionCommitPort::new(Arc::clone(&state), "main");
        let ack = port
            .commit_message(MessageCommit {
                session_id: "session-1".into(),
                turn_id: "turn-1".into(),
                execution_id: "exec-1".into(),
                agent_instance_id: "root".into(),
                message_id: "msg-1".into(),
                parent_message_id: None,
                message: piko_protocol::Message::Assistant {
                    content: vec![ContentBlock::Text { text: "hi".into() }],
                    api: "test".into(),
                    provider: "test".into(),
                    model: "test".into(),
                    usage: None,
                    stop_reason: Some("stop".into()),
                    timestamp: Some(1),
                    error_message: None,
                },
                committed_at: 1,
            })
            .await
            .expect("commit");

        assert_eq!(ack.revision, 1);
        let guard = state.lock().await;
        let session = guard.session("session-1").expect("session");
        assert!(
            session.entries.iter().any(|entry| entry.id() == "msg-1"),
            "expected message projected into HostState"
        );
    }
}
