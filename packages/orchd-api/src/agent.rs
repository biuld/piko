//! Long-lived AgentInstance runtime API.

use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::{
    AgentCancelReceipt, AgentCommitAck, AgentDurableCommand, AgentInboxSnapshot, AgentInputReceipt,
    AgentLifecycleReceipt, AgentLifecycleRequest, AgentSnapshot, CommitError, CreateAgentReceipt,
    CreateAgentRequest, SendAgentInputRequest, SteerAgentRequest,
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
    pub head_message_id: Option<String>,
    pub inbox: Vec<piko_protocol::AgentInboxItem>,
    pub latest_report: Option<piko_protocol::AgentExecutionReport>,
    pub execution_reports: Vec<RecoveredExecutionReport>,
    pub queued_inputs: Vec<piko_protocol::DurableAgentInput>,
    pub pending_detached_deliveries: Vec<RecoveredDetachedDelivery>,
}

#[derive(Debug, Clone)]
pub struct RecoveredDetachedDelivery {
    pub recipient_agent_instance_id: String,
    pub report: piko_protocol::AgentExecutionReport,
}

#[derive(Debug, Clone)]
pub struct RecoveredExecutionReport {
    pub internal_execution_id: String,
    pub report: piko_protocol::AgentExecutionReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAgentHandle {
    pub session_id: String,
    pub root_agent_instance_id: String,
}

/// Mandatory control surface for AgentInstances.
///
/// Execution identity is deliberately absent: callers address Agents and the
/// runtime owns the short-lived Executions used to serve their input.
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

    /// Run one Agent input and await its report without exposing the backing
    /// Execution identity.
    async fn run_agent(
        &self,
        request: SendAgentInputRequest,
    ) -> Result<piko_protocol::AgentExecutionReport, AgentApiError>;

    /// Accept Agent input and durably deliver its eventual report to another
    /// Agent's inbox.
    async fn send_agent_input_detached(
        &self,
        request: SendAgentInputRequest,
        recipient_agent_instance_id: String,
    ) -> Result<AgentInputReceipt, AgentApiError>;

    async fn steer_agent(
        &self,
        request: SteerAgentRequest,
    ) -> Result<AgentInputReceipt, AgentApiError>;

    async fn cancel_agent_run(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentCancelReceipt, AgentApiError>;

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
