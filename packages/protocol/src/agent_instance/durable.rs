use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MailboxWaitRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_agent_instance_id: Option<AgentInstanceId>,
    pub timeout_ms: u64,
    /// Optional single-agent filter; `None` waits on any live agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_instance_id: Option<AgentInstanceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MailboxWaitSummary {
    pub timed_out: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<AgentMailboxEvent>,
    /// Tree-sorted live snapshots at wait completion (see `list_agents`).
    pub agents: Vec<AgentSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentDurableCommand {
    Create {
        identity: AgentInstanceIdentity,
        spec: crate::AgentSpec,
    },
    SetLifecycle {
        agent_instance_id: AgentInstanceId,
        lifecycle: AgentInstanceLifecycle,
    },
    /// Canonical durable admission fact. Rejected proposals never produce
    /// this command; the host commit port is the admission boundary.
    AgentInputAdmitted { admission: AgentInputAdmission },
    /// Canonical durable disposition transition for an admitted input.
    AgentInputDispositionChanged { change: AgentInputDispositionChange },
    /// Durable processing-start fact on the root AgentInput. The root input is
    /// the work identity; there is no Execution aggregate.
    AgentInputProcessingStarted {
        agent_instance_id: AgentInstanceId,
        root_input_id: AgentInputId,
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_turn_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detached_recipient_agent_instance_id: Option<AgentInstanceId>,
        #[serde(default)]
        prompt_assembly_version: u32,
        #[serde(default)]
        prompt_digest: String,
        started_at: i64,
        /// Canonical root input admitted atomically with the processing start.
        input: AgentInput,
    },
    /// Durable processing-finish fact on the root AgentInput.
    AgentInputProcessingFinished {
        agent_instance_id: AgentInstanceId,
        root_input_id: AgentInputId,
        report: AgentWorkReport,
        finished_at: i64,
    },
    CommitReport {
        recipient_agent_instance_id: AgentInstanceId,
        report: AgentWorkReport,
    },
    ConsumeInboxItem {
        agent_instance_id: AgentInstanceId,
        report_id: String,
        consumed_at: i64,
    },
}
