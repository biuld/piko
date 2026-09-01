use std::sync::Mutex;

use async_trait::async_trait;
use piko_orchd_api::ExecutionCommitPort;
use piko_protocol::AgentInputDispositionChange;
use piko_protocol::agent_work::{CommitAck, CommitError, MessageCommit, ModelStepCommit};

/// Test sink for the Execution commit port.
#[derive(Debug, Default)]
pub struct CollectingExecutionCommitPort {
    messages: Mutex<Vec<MessageCommit>>,
    model_steps: Mutex<Vec<ModelStepCommit>>,
    steer_changes: Mutex<Vec<AgentInputDispositionChange>>,
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
            root_input_id: commit.root_input_id.clone(),
            agent_instance_id: commit.agent_instance_id.clone(),
            message_id: Some(commit.message_id.clone()),
            revision,
        };
        self.messages.lock().expect("messages lock").push(commit);
        Ok(ack)
    }

    async fn commit_steer(
        &self,
        message: MessageCommit,
        change: AgentInputDispositionChange,
    ) -> Result<CommitAck, CommitError> {
        self.steer_changes
            .lock()
            .expect("steer changes lock")
            .push(change);
        self.commit_message(message).await
    }

    async fn commit_model_step(&self, commit: ModelStepCommit) -> Result<CommitAck, CommitError> {
        let revision = {
            let mut rev = self.next_revision.lock().expect("revision lock");
            *rev += 1;
            *rev
        };
        let ack = CommitAck {
            session_id: commit.session_id.clone(),
            root_input_id: commit.root_input_id.clone(),
            agent_instance_id: commit.agent_instance_id.clone(),
            message_id: Some(commit.assistant.message_id.clone()),
            revision,
        };
        {
            let mut messages = self.messages.lock().expect("messages lock");
            messages.push(commit.assistant.clone());
            messages.extend(commit.tool_calls.iter().cloned());
        }
        self.model_steps
            .lock()
            .expect("model steps lock")
            .push(commit);
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

    pub fn model_steps(&self) -> Vec<ModelStepCommit> {
        self.model_steps.lock().expect("model steps lock").clone()
    }

    pub fn steer_changes(&self) -> Vec<AgentInputDispositionChange> {
        self.steer_changes
            .lock()
            .expect("steer changes lock")
            .clone()
    }
}
