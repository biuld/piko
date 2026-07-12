//! Execution-path ports and API (single-agent runtime).
//!
//! Parallel to the migration-only Task API. Session capabilities are immutable
//! after attach.

use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::execution::{
    CancelExecutionRequest, CancelReceipt, CommitAck, CommitError, ExecutionInputReceipt,
    ExecutionOutcomeCommit, ExecutionReceipt, ExecutionSnapshot, MessageCommit,
    StartExecutionRequest, SteerExecutionRequest,
};

use crate::error::AgentApiError;
use crate::stream::SessionSubscription;

/// Durable commit port owned by hostd and scoped to a Session/Execution.
#[async_trait]
pub trait ExecutionCommitPort: Send + Sync {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError>;

    async fn commit_execution_outcome(
        &self,
        commit: ExecutionOutcomeCommit,
    ) -> Result<CommitAck, CommitError>;
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

/// Configuration supplied when attaching a Session execution scope.
pub struct SessionExecutionConfig {
    pub session_id: String,
    pub ports: SessionExecutionPorts,
}

/// Handle returned after a successful attach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionExecutionHandle {
    pub session_id: String,
}

/// Target orchd public API for the single-agent Execution path.
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn attach_session(
        &self,
        config: SessionExecutionConfig,
    ) -> Result<SessionExecutionHandle, AgentApiError>;

    async fn detach_session(&self, session_id: String) -> Result<(), AgentApiError>;

    async fn start_execution(
        &self,
        request: StartExecutionRequest,
    ) -> Result<ExecutionReceipt, AgentApiError>;

    async fn steer_execution(
        &self,
        request: SteerExecutionRequest,
    ) -> Result<ExecutionInputReceipt, AgentApiError>;

    async fn request_cancel(
        &self,
        request: CancelExecutionRequest,
    ) -> Result<CancelReceipt, AgentApiError>;

    async fn execution_snapshot(
        &self,
        session_id: String,
        execution_id: String,
    ) -> Result<Option<ExecutionSnapshot>, AgentApiError>;

    /// Optional observation subscription. May be backed by hostd in production;
    /// orchd may expose a testing/local hub during the vertical slice.
    async fn subscribe_session(
        &self,
        request: piko_protocol::agent_runtime::SubscribeRequest,
    ) -> Result<SessionSubscription, AgentApiError>;
}
