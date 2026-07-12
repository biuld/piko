//! Long-lived AgentInstance runtime API.

use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::{
    AgentCommitAck, AgentDurableCommand, AgentInboxSnapshot, AgentInputReceipt,
    AgentLifecycleReceipt, AgentLifecycleRequest, AgentSnapshot, CancelExecutionRequest,
    CancelReceipt, CommitError, CreateAgentReceipt, CreateAgentRequest, SendAgentInputRequest,
    SteerAgentRequest,
};

use crate::{AgentApiError, SessionExecutionPorts};

/// Host-owned durable AgentInstance writer. A successful return means the
/// command is committed, not merely queued.
#[async_trait]
pub trait AgentCommitPort: Send + Sync {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError>;
}

/// Immutable capabilities installed for one SessionAgentScope.
pub struct SessionAgentPorts {
    pub agents: Arc<dyn AgentCommitPort>,
    pub executions: SessionExecutionPorts,
}

/// Attaches the durable root identity and all Session-scoped capabilities.
pub struct SessionAgentConfig {
    pub session_id: String,
    pub root: piko_protocol::AgentInstanceIdentity,
    pub recovered_agents: Vec<AgentRecoveryState>,
    pub ports: SessionAgentPorts,
}

#[derive(Debug, Clone)]
pub struct AgentRecoveryState {
    pub identity: piko_protocol::AgentInstanceIdentity,
    pub spec: piko_protocol::AgentSpec,
    pub lifecycle: piko_protocol::AgentInstanceLifecycle,
    pub transcript: Vec<piko_protocol::Message>,
    pub inbox: Vec<piko_protocol::AgentInboxItem>,
    pub latest_report: Option<piko_protocol::AgentExecutionReport>,
    pub execution_reports: Vec<piko_protocol::AgentExecutionReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAgentHandle {
    pub session_id: String,
    pub root_agent_instance_id: String,
}

/// Mandatory control surface for AgentInstances and their Executions.
#[async_trait]
pub trait AgentRuntimeApi: Send + Sync {
    async fn attach_agent_session(
        &self,
        config: SessionAgentConfig,
    ) -> Result<SessionAgentHandle, AgentApiError>;

    async fn detach_agent_session(&self, session_id: String) -> Result<(), AgentApiError>;

    async fn create_agent(
        &self,
        request: CreateAgentRequest,
    ) -> Result<CreateAgentReceipt, AgentApiError>;

    async fn send_agent_input(
        &self,
        request: SendAgentInputRequest,
    ) -> Result<AgentInputReceipt, AgentApiError>;

    async fn steer_agent(
        &self,
        request: SteerAgentRequest,
    ) -> Result<AgentInputReceipt, AgentApiError>;

    async fn request_cancel_execution(
        &self,
        request: CancelExecutionRequest,
    ) -> Result<CancelReceipt, AgentApiError>;

    async fn close_agent(
        &self,
        request: AgentLifecycleRequest,
    ) -> Result<AgentLifecycleReceipt, AgentApiError>;

    async fn reopen_agent(
        &self,
        request: AgentLifecycleRequest,
    ) -> Result<AgentLifecycleReceipt, AgentApiError>;

    async fn agent_snapshot(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<Option<AgentSnapshot>, AgentApiError>;

    async fn wait_agent_execution(
        &self,
        session_id: String,
        agent_instance_id: String,
        execution_id: String,
    ) -> Result<piko_protocol::AgentExecutionReport, AgentApiError>;

    /// Register durable detached-report delivery. Successful acknowledgement
    /// means tracking is installed; report delivery happens asynchronously.
    async fn track_detached_execution(
        &self,
        session_id: String,
        recipient_agent_instance_id: String,
        source_agent_instance_id: String,
        execution_id: String,
    ) -> Result<(), AgentApiError>;

    async fn list_agents(&self, session_id: String) -> Result<Vec<AgentSnapshot>, AgentApiError>;

    async fn agent_inbox(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentInboxSnapshot, AgentApiError>;

    async fn consume_agent_inbox_item(
        &self,
        request: piko_protocol::ConsumeAgentInboxRequest,
    ) -> Result<piko_protocol::ConsumeAgentInboxReceipt, AgentApiError>;
}
