// ============================================================================
// host-protocol — command DTOs for TUI → hostd
// ============================================================================

use serde::{Deserialize, Serialize};

// ============================================================================
// Unified Rust-side Event types
// ============================================================================

pub use crate::event::{
    AgentId, ApprovalDecision, ApprovalEvent, ApprovalId, ApprovalSnapshot, ApprovalStatus,
    AuthEvent, CommandResult, InteractionAnswer, InteractionChoice, InteractionChoiceId,
    InteractionEvent, InteractionId, InteractionInput, InteractionQuestion, InteractionQuestionId,
    MessageId, MessageRole, ModelEvent, QueueEvent, ServerMessage, SessionId, SessionSnapshot,
    SessionSummary, TaskEvent, TaskId, ToolCallId, ToolCallRef, ToolCallSnapshot, ToolCallStatus,
    ToolEvent, TurnEvent, TurnId, TurnSnapshot, TurnStatus, UserInteractionResponse,
    UserInteractionSnapshot, UserInteractionStatus,
};
pub use crate::messages::{Usage, UsageCost};

pub type CommandId = String;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionListScope {
    CurrentFolder,
    #[default]
    All,
}

// ============================================================================
// Commands (TUI → hostd)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// Set or update an API key for a provider (synchronous).
    AuthSetApiKey {
        command_id: CommandId,
        provider: String,
        api_key: String,
    },
    /// Start OAuth device-code login flow (asynchronous, polling).
    AuthLoginOAuth {
        command_id: CommandId,
        provider: String,
    },
    /// Remove stored credentials for a provider.
    AuthLogout {
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
        #[serde(skip_serializing_if = "Option::is_none")]
        session_path: Option<String>,
    },
    SessionList {
        command_id: CommandId,
        #[serde(default)]
        scope: SessionListScope,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
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
        #[serde(default)]
        summarize: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
    },
    SessionSetLabel {
        command_id: CommandId,
        session_id: SessionId,
        entry_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
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
    UserInteractionRespond {
        command_id: CommandId,
        session_id: SessionId,
        interaction_id: InteractionId,
        response: UserInteractionResponse,
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
    ConfigUpdate {
        command_id: CommandId,
        patch: serde_json::Value,
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
    /// Request the user-visible command catalog from hostd.
    CommandCatalogGet {
        command_id: CommandId,
    },
    /// Manually trigger session compaction (bypasses auto threshold).
    SessionCompact {
        command_id: CommandId,
        session_id: SessionId,
    },
    /// Get settings under a namespace (e.g. "tui").
    ConfigGet {
        command_id: CommandId,
        namespace: String,
    },
}

impl Command {
    pub fn command_id(&self) -> &str {
        match self {
            Self::AuthSetApiKey { command_id, .. }
            | Self::AuthLoginOAuth { command_id, .. }
            | Self::AuthLogout { command_id, .. }
            | Self::SessionCreate { command_id, .. }
            | Self::SessionOpen { command_id, .. }
            | Self::SessionList { command_id, .. }
            | Self::SessionFork { command_id, .. }
            | Self::SessionImport { command_id, .. }
            | Self::SessionRename { command_id, .. }
            | Self::SessionDelete { command_id, .. }
            | Self::SessionNavigate { command_id, .. }
            | Self::SessionSetLabel { command_id, .. }
            | Self::TurnSubmit { command_id, .. }
            | Self::TurnCancel { command_id, .. }
            | Self::ApprovalRespond { command_id, .. }
            | Self::UserInteractionRespond { command_id, .. }
            | Self::StateSnapshot { command_id, .. }
            | Self::EventsResume { command_id, .. }
            | Self::ConfigUpdate { command_id, .. }
            | Self::QueueSteer { command_id, .. }
            | Self::QueueFollowUp { command_id, .. }
            | Self::QueueNextTurn { command_id, .. }
            | Self::ModelList { command_id }
            | Self::CommandCatalogGet { command_id }
            | Self::SessionCompact { command_id, .. }
            | Self::ConfigGet { command_id, .. } => command_id,
        }
    }
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
