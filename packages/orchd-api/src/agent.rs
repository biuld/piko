//! Long-lived AgentInstance runtime API.

use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::{
    AgentCancelReceipt, AgentCommitAck, AgentDurableCommand, AgentInboxSnapshot, AgentInput,
    AgentInputCancelReceipt, AgentInputReceipt, AgentInterruptReceipt, AgentLifecycleReceipt,
    AgentLifecycleRequest, AgentSnapshot, CommitError, CreateAgentReceipt, CreateAgentRequest,
    SendAgentInputRequest, SteerAgentRequest,
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
    pub latest_report: Option<piko_protocol::AgentRunReport>,
    pub execution_reports: Vec<RecoveredExecutionReport>,
    pub queued_inputs: Vec<piko_protocol::AgentInput>,
    pub pending_detached_deliveries: Vec<RecoveredDetachedDelivery>,
}

#[derive(Debug, Clone)]
pub struct RecoveredDetachedDelivery {
    pub recipient_agent_instance_id: String,
    pub report: piko_protocol::AgentRunReport,
}

#[derive(Debug, Clone)]
pub struct RecoveredExecutionReport {
    pub internal_execution_id: String,
    pub report: piko_protocol::AgentRunReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAgentHandle {
    pub session_id: String,
    pub root_agent_instance_id: String,
}

pub struct AgentRunAcceptance {
    pub receipt: AgentInputReceipt,
    pub started: piko_comms::ReplyReceiver<piko_comms::contracts::AgentRunStarted, ()>,
    pub completion: piko_comms::ReplyReceiver<
        piko_comms::contracts::AgentRunReport,
        Result<piko_protocol::AgentRunReport, AgentApiError>,
    >,
}

impl AgentRunAcceptance {
    pub async fn wait_started(&mut self) -> Result<(), AgentApiError> {
        (&mut self.started)
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)
    }

    pub async fn wait(self) -> Result<piko_protocol::AgentRunReport, AgentApiError> {
        self.completion
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?
    }
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

    /// Canonical AgentInput admission entry point. The compatibility request
    /// adapter is retained until all callers provide primitive proposals.
    async fn submit_agent_input(
        &self,
        input: AgentInput,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        let input_id = input.input_id.clone();
        let mut receipt = self.send_agent_input(input.to_request()).await?;
        // Compatibility implementations historically used request_id as the
        // input identity. The concrete orchd implementation overrides this
        // method and preserves distinct IDs; this mapping keeps older trait
        // adapters truthful for the initial equal-ID migration case.
        if input_id != receipt.request_id {
            return Err(AgentApiError::IdempotencyConflict);
        }
        receipt.input_id = input_id;
        Ok(receipt)
    }

    /// Accept one Agent input and return its durable receipt plus start and
    /// completion signals without exposing the backing Execution identity.
    async fn run_agent(
        &self,
        request: SendAgentInputRequest,
    ) -> Result<AgentRunAcceptance, AgentApiError>;

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

    /// Agent-addressed interrupt that does not require a host Turn.
    async fn interrupt_agent(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentInterruptReceipt, AgentApiError> {
        let receipt = self
            .cancel_agent_run(session_id.clone(), agent_instance_id.clone())
            .await?;
        Ok(AgentInterruptReceipt {
            session_id,
            agent_instance_id,
            accepted: receipt.accepted,
        })
    }

    async fn cancel_agent_input(
        &self,
        session_id: String,
        agent_instance_id: String,
        request_id: String,
    ) -> Result<AgentCancelReceipt, AgentApiError>;

    /// Canonical input-ID cancellation adapter. Older runtimes used the
    /// request ID as their queue identity, so the compatibility method is
    /// used until the actor stores distinct input IDs end to end.
    async fn cancel_agent_input_id(
        &self,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<AgentInputCancelReceipt, AgentApiError> {
        let receipt = self
            .cancel_agent_input(
                session_id.clone(),
                agent_instance_id.clone(),
                input_id.clone(),
            )
            .await?;
        Ok(AgentInputCancelReceipt {
            input_id,
            request_id: receipt.request_id,
            session_id,
            agent_instance_id,
            accepted: receipt.accepted,
        })
    }

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

    /// Registered AgentSpec templates available for spawn (F-21). Session-independent.
    async fn list_agent_specs(&self) -> Result<Vec<piko_protocol::AgentSpec>, AgentApiError>;

    async fn agent_inbox(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentInboxSnapshot, AgentApiError>;

    async fn consume_agent_inbox_item(
        &self,
        request: piko_protocol::ConsumeAgentInboxRequest,
    ) -> Result<piko_protocol::ConsumeAgentInboxReceipt, AgentApiError>;

    /// Wait (bounded by `timeout_ms`) for the next mailbox notification in the
    /// session. Observational: never writes or consumes durable state.
    async fn wait_agent_mailbox(
        &self,
        request: piko_protocol::MailboxWaitRequest,
    ) -> Result<piko_protocol::MailboxWaitSummary, AgentApiError>;
}
