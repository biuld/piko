//! ExecutionCommitPort that fans out SessionOutput events after durable ack.

use std::sync::Arc;

use async_trait::async_trait;
use orchd::testing::SessionOutputHub;
use orchd_api::ExecutionCommitPort;
use piko_protocol::MessageRole;
use piko_protocol::agent_runtime::{SessionEvent, SessionEventEnvelope};
use piko_protocol::execution::{CommitAck, CommitError, ExecutionOutcomeCommit, MessageCommit};

pub struct NotifyingExecutionCommitPort {
    inner: Arc<dyn ExecutionCommitPort>,
    hub: Arc<SessionOutputHub>,
    agent_id: String,
    /// Root transcript shard id used for MessageCommitted observation / projection.
    storage_task_id: String,
}

impl NotifyingExecutionCommitPort {
    pub fn new(
        inner: Arc<dyn ExecutionCommitPort>,
        hub: Arc<SessionOutputHub>,
        agent_id: impl Into<String>,
        storage_task_id: impl Into<String>,
    ) -> Self {
        Self {
            inner,
            hub,
            agent_id: agent_id.into(),
            storage_task_id: storage_task_id.into(),
        }
    }
}

#[async_trait]
impl ExecutionCommitPort for NotifyingExecutionCommitPort {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError> {
        let role = match &commit.message {
            piko_protocol::Message::User { .. } => MessageRole::User,
            piko_protocol::Message::Assistant { .. } => MessageRole::Assistant,
            piko_protocol::Message::ToolCall { .. } | piko_protocol::Message::ToolResult { .. } => {
                MessageRole::ToolResult
            }
        };
        let ack = self.inner.commit_message(commit.clone()).await?;
        let _ = self
            .hub
            .publish_event(SessionEventEnvelope {
                agent_instance_id: commit.agent_instance_id,
                task_id: self.storage_task_id.clone(),
                agent_id: self.agent_id.clone(),
                task_seq: ack.revision,
                cursor: self.hub.cursor(),
                event: SessionEvent::MessageCommitted {
                    message_id: commit.message_id,
                    work_id: commit.turn_id,
                    role,
                },
            })
            .await;
        Ok(ack)
    }

    async fn commit_execution_outcome(
        &self,
        commit: ExecutionOutcomeCommit,
    ) -> Result<CommitAck, CommitError> {
        self.inner.commit_execution_outcome(commit).await
    }
}
