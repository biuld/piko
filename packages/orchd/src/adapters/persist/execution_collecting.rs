use std::sync::Mutex;

use async_trait::async_trait;
use piko_orchd_api::ExecutionCommitPort;
use piko_protocol::execution::{CommitAck, CommitError, MessageCommit};

/// Test sink for the Execution commit port.
#[derive(Debug, Default)]
pub struct CollectingExecutionCommitPort {
    messages: Mutex<Vec<MessageCommit>>,
    next_revision: Mutex<u64>,
}

#[async_trait]
impl ExecutionCommitPort for CollectingExecutionCommitPort {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError> {
        let revision = {
            let mut rev = self.next_revision.lock().expect("revision lock");
            *rev += 1;
            *rev
        };
        let ack = CommitAck {
            session_id: commit.session_id.clone(),
            execution_id: commit.execution_id.clone(),
            agent_instance_id: commit.agent_instance_id.clone(),
            message_id: Some(commit.message_id.clone()),
            revision,
        };
        self.messages.lock().expect("messages lock").push(commit);
        Ok(ack)
    }
}

impl CollectingExecutionCommitPort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn messages(&self) -> Vec<MessageCommit> {
        self.messages.lock().expect("messages lock").clone()
    }
}
