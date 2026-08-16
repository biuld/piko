//! Session storage DTOs and errors shared by `ports::session_store` and
//! `ports::session_repository`.
//!
//! These read models live at the port boundary so `application` can query the
//! journal aggregate without depending on `crate::infra` or `crate::adapters`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use piko_protocol::{
    AgentInboxItem, AgentInstanceIdentity, AgentInstanceLifecycle, AgentRunReport, Message,
    SessionTreeEntry,
};
use serde::{Deserialize, Serialize};

use crate::domain::prompts::WorldStateFacts;
use crate::domain::sessions::{SessionModelRef, SessionState};

/// One raw journal event surfaced to application-level observational readers
/// (trajectory query). Optional event types are included; the acknowledged
/// projection ignores them.
#[derive(Debug, Clone, PartialEq)]
pub struct RawJournalEventRef {
    pub revision: u64,
    pub committed_at: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionProjection {
    pub session_id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub current_leaf_id: Option<String>,
    pub selected_agent_instance_id: Option<String>,
    pub root_agent_instance_id: Option<String>,
    pub journal_revision: u64,
    pub agents: BTreeMap<String, AgentProjection>,
    pub agent_inbox: Vec<AgentInboxItem>,
    pub agent_executions: BTreeMap<String, ExecutionProjection>,
    pub agent_input_queue: Vec<piko_protocol::DurableAgentInput>,
    /// Session-scoped model continuity record: the provider+model that
    /// executed the most recent turn. Derives the durable `ModelChange`
    /// marker and the prompt model-switch fragment.
    pub last_model: Option<SessionModelRef>,
    /// World-state diff baseline for the next root turn (F-04 slice 2).
    /// Cleared by compaction so the next run re-injects the full snapshot.
    pub world_state_baseline: Option<WorldStateFacts>,
    /// Session-scoped metadata only; transcript messages never live here.
    pub entries: Vec<SessionTreeEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentProjection {
    pub identity: AgentInstanceIdentity,
    pub spec: Option<piko_protocol::AgentSpec>,
    pub lifecycle: AgentInstanceLifecycle,
    pub latest_report: Option<AgentRunReport>,
    /// Durable current todo list for this agent (F-27).
    pub todo_list: Option<piko_protocol::TodoList>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionProjection {
    pub agent_instance_id: String,
    pub run_id: String,
    pub execution_id: String,
    pub request_id: String,
    pub source_turn_id: Option<String>,
    pub detached_recipient_agent_instance_id: Option<String>,
    pub detached_report_delivered: bool,
    pub prompt_assembly_version: u32,
    pub prompt_digest: String,
    pub status: piko_protocol::ExecutionStatus,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub report: Option<AgentRunReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommittedMessage {
    pub id: String,
    /// Parent in this AgentInstance's private transcript chain.
    pub parent_id: Option<String>,
    /// Parent in the session-wide navigable tree.
    #[serde(default)]
    pub tree_parent_id: Option<String>,
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
