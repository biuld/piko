//! Session storage DTOs and errors shared by `ports::session_store` and
//! `ports::session_repository`.
//!
//! These types were historically owned by `infra::storage`; they live here
//! so `application` can depend on the storage port surface without pulling
//! in `crate::infra` or `crate::adapters`. `infra::storage` re-exports the
//! same names for backward-compatible call sites (tests, adapters).

use std::collections::BTreeMap;
use std::path::PathBuf;

use piko_protocol::{
    AgentInboxItem, AgentInstanceIdentity, AgentInstanceLifecycle, AgentRunReport, Message,
    SessionTreeEntry,
};
use serde::{Deserialize, Serialize};

use crate::domain::sessions::SessionState;

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
    pub latest_report: Option<AgentRunReport>,
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
    pub report: Option<AgentRunReport>,
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

#[derive(Debug, Clone)]
pub struct RecoveredAgent {
    pub session_id: String,
    pub agent_instance_id: String,
    pub agent_spec_id: String,
    pub transcript: Vec<CommittedMessage>,
    pub head_message_id: Option<String>,
    pub last_transcript_seq: u64,
}

/// A loaded session with its in-memory state and file path.
#[derive(Debug, Clone)]
pub struct PersistedSession {
    pub state: SessionState,
    pub path: PathBuf,
    pub created_at: String,
    pub parent_session_path: Option<String>,
}

/// Errors that can occur during session storage operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionStorageError {
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("invalid session {path}: {message}")]
    Invalid { path: PathBuf, message: String },
    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("json error for {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}
