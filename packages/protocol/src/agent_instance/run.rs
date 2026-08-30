use serde::{Deserialize, Serialize};

use super::*;
use crate::{ExecutionOutcome, MessageContent, Usage};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkReport {
    pub agent_instance_id: AgentInstanceId,
    pub root_input_id: AgentInputId,
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
    pub latest_report: Option<AgentWorkReport>,
    /// Root AgentInput currently starting or running, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_root_input_id: Option<crate::AgentInputId>,
    /// Follow-ups admitted but not yet applied as root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_follow_up_ids: Vec<crate::AgentInputId>,
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
    pub root_input_id: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputReceipt {
    #[serde(default)]
    pub input_id: AgentInputId,
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub disposition: crate::AgentInputDisposition,
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
    pub report: AgentWorkReport,
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
