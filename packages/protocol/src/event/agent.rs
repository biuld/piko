use serde::{Deserialize, Serialize};

use super::*;
use crate::AgentStatus;

pub type SessionId = String;
pub type MessageId = String;
pub type ToolCallId = String;
pub type ApprovalId = String;
pub type InteractionId = String;
pub type InteractionQuestionId = String;
pub type InteractionChoiceId = String;
pub type AgentId = String;

/// Agent status information maintained by hostd; the TUI queries it through AgentList.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub session_id: SessionId,
    pub agent_instance_id: crate::AgentInstanceId,
    pub agent_id: AgentId,
    pub parent_agent_instance_id: Option<crate::AgentInstanceId>,
    pub lifecycle: crate::AgentInstanceLifecycle,
    pub activity: crate::AgentActivity,
    pub unread_report_count: u32,
    pub name: String,
    pub role: String,
    pub status: AgentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequencedServerMessage {
    pub seq: u64,
    pub message: Box<ServerMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentViewSnapshot {
    pub agent_instance_id: crate::AgentInstanceId,
    pub agent_id: AgentId,
    pub parent_agent_instance_id: Option<crate::AgentInstanceId>,
    pub status: Option<AgentStatus>,
    pub next_seq: u64,
    pub events: Vec<SequencedServerMessage>,
}
