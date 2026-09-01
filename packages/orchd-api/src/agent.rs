//! Long-lived AgentInstance runtime API.

use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::{
    AgentCommitAck, AgentDurableCommand, AgentInboxSnapshot, AgentInput, AgentInputCancelReceipt,
    AgentInputReceipt, AgentInterruptReceipt, AgentLifecycleReceipt, AgentLifecycleRequest,
    AgentSnapshot, CommitError, CreateAgentReceipt, CreateAgentRequest, PromptResourceSnapshot,
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
    pub latest_report: Option<piko_protocol::AgentWorkReport>,
    pub execution_reports: Vec<RecoveredExecutionReport>,
    pub queued_inputs: Vec<piko_protocol::AgentInput>,
    pub pending_detached_deliveries: Vec<RecoveredDetachedDelivery>,
}

#[derive(Debug, Clone)]
pub struct RecoveredDetachedDelivery {
    pub recipient_agent_instance_id: String,
    pub report: piko_protocol::AgentWorkReport,
}

#[derive(Debug, Clone)]
pub struct RecoveredExecutionReport {
    pub root_input_id: String,
    pub report: piko_protocol::AgentWorkReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAgentHandle {
    pub session_id: String,
    pub root_agent_instance_id: String,
}

/// Live extras for one AgentInput admission. Not part of the durable fact.
#[derive(Debug, Clone, Default)]
pub struct AgentInputRuntime {
    pub prompt_resources: Option<PromptResourceSnapshot>,
    pub active_tool_names: Option<Vec<String>>,
    pub root_input_id: Option<String>,
    pub message_id: Option<String>,
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

    /// Canonical and only AgentInput admission entry point.
    async fn submit_agent_input(
        &self,
        input: AgentInput,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        self.submit_runtime_agent_input(input, AgentInputRuntime::default())
            .await
    }

    /// Admit an AgentInput with host-private runtime extras (prompt staging,
    /// tool restriction). Durable facts stay on `input`.
    async fn submit_runtime_agent_input(
        &self,
        input: AgentInput,
        runtime: AgentInputRuntime,
    ) -> Result<AgentInputReceipt, AgentApiError>;

    /// Accept Agent input and durably deliver its eventual report to another
    /// Agent's inbox.
    async fn submit_agent_input_detached(
        &self,
        input: AgentInput,
        recipient_agent_instance_id: String,
    ) -> Result<AgentInputReceipt, AgentApiError>;

    /// Agent-addressed interrupt of the active root. Does not require a host
    /// Turn and cannot address a successor root.
    async fn interrupt_agent(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentInterruptReceipt, AgentApiError>;

    /// Cancel exactly one pending input by its durable control identity.
    async fn cancel_agent_input(
        &self,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<AgentInputCancelReceipt, AgentApiError>;

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

    /// Wait until `input_id` is the active root, has already produced a
    /// report, or is no longer a pending follow-up (cancelled).
    async fn wait_agent_input_started(
        &self,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<(), AgentApiError>;

    /// Observe the durable terminal report for one root input. This is a
    /// latest-state query, not a second admission or work handle.
    async fn wait_agent_input_completion(
        &self,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<piko_protocol::AgentWorkReport, AgentApiError>;

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
