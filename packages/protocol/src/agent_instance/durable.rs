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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin_root_input_id: Option<AgentInputId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin_tool_call_id: Option<String>,
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
        detached_recipient_agent_instance_id: Option<AgentInstanceId>,
        #[serde(default)]
        prompt_assembly_version: u32,
        #[serde(default)]
        prompt_digest: String,
        started_at: i64,
        /// Canonical root input admitted atomically with the processing start.
        input: AgentInput,
        /// Transcript identity of the initiating user message. Its application
        /// is committed in the same semantic commit as admission/start.
        input_message_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_parent_message_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_tree_parent_entry_id: Option<String>,
        input_committed_at: i64,
    },
    /// Durable processing-finish fact on the root AgentInput.
    AgentInputProcessingFinished {
        agent_instance_id: AgentInstanceId,
        root_input_id: AgentInputId,
        report: AgentWorkReport,
        finished_at: i64,
    },
    /// Durable user-action request attached to the active root input.
    PendingActionRequested {
        agent_instance_id: AgentInstanceId,
        root_input_id: AgentInputId,
        action: PendingActionSummary,
        requested_at: i64,
    },
    /// Durable resolution of a previously requested user action.
    PendingActionResolved {
        agent_instance_id: AgentInstanceId,
        root_input_id: AgentInputId,
        action_id: String,
        resolved_at: i64,
    },
    /// Durable interrupt intent for the active root input. Processing finish
    /// remains the terminal fact that clears the active work.
    InterruptRequested {
        agent_instance_id: AgentInstanceId,
        root_input_id: AgentInputId,
        requested_at: i64,
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
