use std::collections::BTreeMap;

use piko_protocol::{
    AgentExecutionReport, AgentInboxItem, AgentInstanceIdentity, AgentInstanceLifecycle, Message,
    SessionTreeEntry,
};
use serde::{Deserialize, Serialize};

pub const SESSION_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionManifest {
    pub schema_version: u32,
    pub session_id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub current_leaf_id: Option<String>,
    #[serde(default)]
    pub root_agent_instance_id: Option<String>,
    #[serde(default)]
    pub agent_revision: u64,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentManifestEntry>,
    #[serde(default)]
    pub agent_inbox: Vec<AgentInboxItem>,
    #[serde(default)]
    pub agent_executions: BTreeMap<String, AgentExecutionManifestEntry>,
    #[serde(default)]
    pub agent_input_queue: Vec<piko_protocol::DurableAgentInput>,
    /// Session-scoped metadata only; transcript messages never live here.
    pub entries: Vec<SessionTreeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentManifestEntry {
    pub identity: AgentInstanceIdentity,
    #[serde(default)]
    pub spec: Option<piko_protocol::AgentSpec>,
    pub lifecycle: AgentInstanceLifecycle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_report: Option<AgentExecutionReport>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionManifestEntry {
    pub agent_instance_id: String,
    pub run_id: String,
    pub execution_id: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    #[serde(default)]
    pub detached_recipient_agent_instance_id: Option<String>,
    #[serde(default)]
    pub detached_report_delivered: bool,
    pub status: piko_protocol::ExecutionStatus,
    pub started_at: i64,
    #[serde(default)]
    pub finished_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<AgentExecutionReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentShardHeader {
    pub schema_version: u32,
    pub session_id: String,
    pub agent_instance_id: String,
    pub agent_spec_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommittedMessage {
    pub id: String,
    pub parent_id: Option<String>,
    pub agent_instance_id: String,
    pub agent_spec_id: String,
    #[serde(default)]
    pub execution_id: Option<String>,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    pub transcript_seq: u64,
    pub timestamp: i64,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub(super) enum AgentShardRecord {
    Header(AgentShardHeader),
    Message(CommittedMessage),
}

#[derive(Debug, Clone)]
pub struct RecoveredAgent {
    pub session_id: String,
    pub agent_instance_id: String,
    pub agent_spec_id: String,
    pub transcript: Vec<CommittedMessage>,
    pub head_message_id: Option<String>,
    pub last_transcript_seq: u64,
}
