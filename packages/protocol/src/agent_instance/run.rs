use serde::{Deserialize, Serialize};

use super::*;
use crate::{ExecutionOutcome, MessageContent, Usage};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunReport {
    pub agent_instance_id: AgentInstanceId,
    pub report_id: String,
    pub outcome: ExecutionOutcome,
    pub summary: String,
    pub usage: Usage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<AgentArtifactRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentArtifactRef {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSnapshot {
    pub identity: AgentInstanceIdentity,
    pub lifecycle: AgentInstanceLifecycle,
    pub activity: AgentActivity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_report: Option<AgentRunReport>,
    pub unread_report_count: u32,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentInputDelivery {
    Auto,
    StartWhenIdle,
    SteerActive,
    FollowUp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentRequest {
    pub request_id: String,
    pub session_id: String,
    pub parent_agent_instance_id: AgentInstanceId,
    pub agent_spec_id: AgentSpecId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_agent_instance_id: Option<AgentInstanceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentReceipt {
    pub request_id: String,
    pub identity: AgentInstanceIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SendAgentInputRequest {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_agent_instance_id: Option<AgentInstanceId>,
    /// Interaction Turn this input is bound to. `Some` on the root Turn path,
    /// `None` for child agent runs spawned by multi-agent tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
    pub message_id: String,
    pub content: MessageContent,
    pub delivery: AgentInputDelivery,
    /// Trusted host-owned prompt resources for this run. Child/tool callers
    /// omit this and receive the AgentSpec base prompt plus resolved tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_resources: Option<crate::PromptResourceSnapshot>,
    /// Optional transient restriction intersected with the AgentSpec allow-list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_tool_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteerAgentRequest {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_agent_instance_id: Option<AgentInstanceId>,
    pub message_id: String,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputReceipt {
    #[serde(default)]
    pub input_id: AgentInputId,
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub disposition: crate::InputDisposition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queued_position: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInterruptReceipt {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputCancelReceipt {
    pub input_id: AgentInputId,
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub accepted: bool,
}

/// Durable follow-up input owned by one AgentInstance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DurableAgentInput {
    pub queued_input_id: String,
    pub request: SendAgentInputRequest,
    /// Canonical submission time when this compatibility queue row was
    /// created from an AgentInput. Legacy queue rows omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detached_recipient_agent_instance_id: Option<AgentInstanceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLifecycleRequest {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_agent_instance_id: Option<AgentInstanceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLifecycleReceipt {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub lifecycle: AgentInstanceLifecycle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInboxItem {
    pub report_id: String,
    pub recipient_agent_instance_id: AgentInstanceId,
    pub source_agent_instance_id: AgentInstanceId,
    pub report: AgentRunReport,
    pub committed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInboxSnapshot {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub items: Vec<AgentInboxItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeAgentInboxRequest {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub report_id: String,
    pub consumed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeAgentInboxReceipt {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub report_id: String,
    pub consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommitAck {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCancelReceipt {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub accepted: bool,
}
