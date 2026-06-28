// ============================================================================
// host-protocol — unified wire protocol for hostd ↔ TUI ↔ orchd
//
// Domain events (persisted to JSONL, used for state rebuild): 15 types
// Streaming events (real-time only, never persisted): 8 types
//
// Every domain event carries session_id + timestamp.
// Every streaming event carries task_id + agent_id.
// ============================================================================

use serde::{Deserialize, Serialize};

// ============================================================================
// Unified Rust-side Event types
// ============================================================================

pub use crate::event::{
    AgentId, ApprovalDecision, ApprovalId, ApprovalSnapshot, ApprovalStatus, Event, MessageId,
    MessageRole, SessionId, SessionSnapshot, SessionSummary, TaskId, ToolCallId, ToolCallRef,
    ToolCallSnapshot, ToolCallStatus, TurnId, TurnSnapshot, TurnStatus,
};
pub use crate::messages::{Usage, UsageCost};

pub type CommandId = String;

// ============================================================================
// Commands (TUI → hostd)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    AuthLoginStart {
        command_id: CommandId,
        provider: String,
    },
    SessionCreate {
        command_id: CommandId,
        cwd: String,
    },
    SessionOpen {
        command_id: CommandId,
        session_id: SessionId,
    },
    SessionList {
        command_id: CommandId,
    },
    SessionFork {
        command_id: CommandId,
        session_id: SessionId,
        #[serde(skip_serializing_if = "Option::is_none")]
        entry_id: Option<String>,
    },
    SessionImport {
        command_id: CommandId,
        path: String,
    },
    SessionRename {
        command_id: CommandId,
        session_id: SessionId,
        name: String,
    },
    SessionDelete {
        command_id: CommandId,
        session_id: SessionId,
    },
    SessionNavigate {
        command_id: CommandId,
        session_id: SessionId,
        entry_id: String,
    },
    TurnSubmit {
        command_id: CommandId,
        session_id: SessionId,
        text: String,
    },
    TurnCancel {
        command_id: CommandId,
        session_id: SessionId,
        turn_id: TurnId,
    },
    ApprovalRespond {
        command_id: CommandId,
        session_id: SessionId,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    StateSnapshot {
        command_id: CommandId,
        session_id: SessionId,
    },
    EventsResume {
        command_id: CommandId,
        session_id: SessionId,
        after_seq: u64,
    },
    ConfigSet {
        command_id: CommandId,
        #[serde(skip_serializing_if = "Option::is_none")]
        default_provider: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default_thinking_level: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        active_tools: Option<Vec<String>>,
    },
    /// Push a steering message into the session's queue.
    QueueSteer {
        command_id: CommandId,
        session_id: SessionId,
        task_id: TaskId,
        message: String,
    },
    QueueFollowUp {
        command_id: CommandId,
        session_id: SessionId,
        message: String,
    },
    QueueNextTurn {
        command_id: CommandId,
        session_id: SessionId,
        message: String,
    },
    /// Request the list of available models from hostd's catalog.
    ModelList {
        command_id: CommandId,
    },
    /// Manually trigger session compaction (bypasses auto threshold).
    SessionCompact {
        command_id: CommandId,
        session_id: SessionId,
    },
}

impl Command {
    pub fn command_id(&self) -> &str {
        match self {
            Self::AuthLoginStart { command_id, .. }
            | Self::SessionCreate { command_id, .. }
            | Self::SessionOpen { command_id, .. }
            | Self::SessionList { command_id }
            | Self::SessionFork { command_id, .. }
            | Self::SessionImport { command_id, .. }
            | Self::SessionRename { command_id, .. }
            | Self::SessionDelete { command_id, .. }
            | Self::SessionNavigate { command_id, .. }
            | Self::TurnSubmit { command_id, .. }
            | Self::TurnCancel { command_id, .. }
            | Self::ApprovalRespond { command_id, .. }
            | Self::StateSnapshot { command_id, .. }
            | Self::EventsResume { command_id, .. }
            | Self::ConfigSet { command_id, .. }
            | Self::QueueSteer { command_id, .. }
            | Self::QueueFollowUp { command_id, .. }
            | Self::QueueNextTurn { command_id, .. }
            | Self::ModelList { command_id }
            | Self::SessionCompact { command_id, .. } => command_id,
        }
    }
}

// ============================================================================
// Command acks (not domain events)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandAck {
    CommandAccepted {
        command_id: CommandId,
    },
    CommandRejected {
        command_id: CommandId,
        reason: String,
    },
}

impl CommandAck {
    pub fn command_id(&self) -> &str {
        match self {
            Self::CommandAccepted { command_id } | Self::CommandRejected { command_id, .. } => {
                command_id
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResumeResponse {
    Events { events: Vec<Event> },
    Snapshot { event: Box<Event> },
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum ProtocolError {
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("turn not found: {0}")]
    TurnNotFound(String),
    #[error("approval not found: {0}")]
    ApprovalNotFound(String),
    #[error("session already has an active turn: {0}")]
    ActiveTurnExists(String),
    #[error("invalid command: {0}")]
    InvalidCommand(String),
}
