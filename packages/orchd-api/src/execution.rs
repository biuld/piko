//! Internal Execution capabilities supplied by hostd to AgentRuntime.

use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::execution::{CommitAck, CommitError, MessageCommit};

use crate::AgentApiError;

/// Durable commit port owned by hostd and scoped to a Session/Execution.
#[async_trait]
pub trait ExecutionCommitPort: Send + Sync {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError>;
}

/// Approval requests addressed by session + execution identity.
#[async_trait]
pub trait ApprovalPort: Send + Sync {
    async fn request_approval(
        &self,
        session_id: &str,
        execution_id: &str,
        request: crate::ToolApprovalRequest,
    ) -> Result<crate::ToolApprovalDecision, AgentApiError>;
}

/// Interactive prompts addressed by session + execution identity.
#[async_trait]
pub trait InteractionPort: Send + Sync {
    async fn request_interaction(
        &self,
        session_id: &str,
        execution_id: &str,
        request: serde_json::Value,
    ) -> Result<serde_json::Value, AgentApiError>;
}

/// Lossy realtime fan-out. Must never block the ExecutionActor.
pub trait RealtimeDeltaSink: Send + Sync {
    fn try_publish(&self, delta: piko_protocol::agent_runtime::RealtimeDeltaEnvelope);
}

/// Immutable session-scoped capabilities for one attached Session.
pub struct SessionExecutionPorts {
    pub commit: Arc<dyn ExecutionCommitPort>,
    pub approval: Option<Arc<dyn ApprovalPort>>,
    pub interaction: Option<Arc<dyn InteractionPort>>,
    pub realtime: Option<Arc<dyn RealtimeDeltaSink>>,
}

impl SessionExecutionPorts {
    pub fn new(commit: Arc<dyn ExecutionCommitPort>) -> Self {
        Self {
            commit,
            approval: None,
            interaction: None,
            realtime: None,
        }
    }

    pub fn with_approval(mut self, approval: Arc<dyn ApprovalPort>) -> Self {
        self.approval = Some(approval);
        self
    }

    pub fn with_interaction(mut self, interaction: Arc<dyn InteractionPort>) -> Self {
        self.interaction = Some(interaction);
        self
    }

    pub fn with_realtime(mut self, realtime: Arc<dyn RealtimeDeltaSink>) -> Self {
        self.realtime = Some(realtime);
        self
    }
}
