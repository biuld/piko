use std::sync::Arc;

use orchd_api::{AgentApiError, ExecutionCommitPort};
use piko_protocol::{Message, execution::MessageCommit};

use crate::runtime::execution::{ExecutionIdentity, state::ExecutionState};

/// A message has no capability to mutate reusable Execution state until its
/// durable commit succeeds.
pub(crate) struct MessageCommitScope {
    commit: MessageCommit,
}

pub(crate) struct CommittedMessage {
    message_id: String,
    message: Message,
}

impl MessageCommitScope {
    pub fn new(
        identity: &ExecutionIdentity,
        parent_message_id: Option<String>,
        message_id: String,
        message: Message,
    ) -> Self {
        Self {
            commit: MessageCommit {
                session_id: identity.session_id.clone(),
                source_turn_id: identity.source_turn_id.clone(),
                execution_id: identity.execution_id.clone(),
                agent_instance_id: identity.agent_instance_id.clone(),
                message_id,
                parent_message_id,
                message,
                committed_at: chrono::Utc::now().timestamp_millis(),
            },
        }
    }

    pub async fn commit(
        self,
        port: &Arc<dyn ExecutionCommitPort>,
    ) -> Result<CommittedMessage, AgentApiError> {
        port.commit_message(self.commit.clone())
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        Ok(CommittedMessage {
            message_id: self.commit.message_id,
            message: self.commit.message,
        })
    }
}

impl CommittedMessage {
    pub fn apply(self, state: &mut ExecutionState) {
        state.head_message_id = Some(self.message_id);
        state.transcript.push_message(self.message);
    }
}
