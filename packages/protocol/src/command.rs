// ============================================================================
// host-protocol — command DTOs for TUI → hostd
// ============================================================================

use serde::{Deserialize, Serialize};

// ============================================================================
// Unified Rust-side Event types
// ============================================================================

pub use crate::event::{
    AgentId, AgentInfo, AgentRunEvent, ApprovalDecision, ApprovalEvent, ApprovalId,
    ApprovalSnapshot, ApprovalStatus, AuthEvent, CommandResult, InteractionAnswer,
    InteractionChoice, InteractionChoiceId, InteractionId, InteractionInput, InteractionQuestion,
    InteractionQuestionId, LifecycleEvent, MessageId, MessageRole, ModelEvent, QueueEvent,
    ServerMessage, SessionId, SessionSnapshot, SessionSummary, ToolCallId, ToolCallRef,
    ToolCallSnapshot, ToolCallStatus, TurnEvent, TurnId, TurnSnapshot, TurnStatus,
    UserInteractionResponse, UserInteractionSnapshot, UserInteractionStatus,
};
pub use crate::messages::{Usage, UsageCost};

pub type CommandId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OAuthLoginMode {
    #[default]
    Browser,
    DeviceCode,
}

/// A currently running external process spawned by the workspace `process`
/// tool (F-08). Modeled on codex-rs `BackgroundTerminalInfo`, plus piko's
/// exit state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    /// Provider-local id (`proc-N`) used by the `process` tool.
    pub process_id: String,
    /// OS process id of the session-leading shell.
    pub pid: u32,
    /// The command line the process was started with.
    pub command: String,
    /// Working directory the process was started in.
    pub cwd: String,
    /// Whether the process has exited.
    pub exited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
}

/// Exit status returned when a process is terminated.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessExit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
}

/// MCP server status snapshot for the `mcp.status` command (F-13 TUI
/// surface). One entry per configured server; `connected` distinguishes live
/// providers from failed/timed-out connects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    /// Configured server name (also the provider/tool-set id).
    pub name: String,
    pub connected: bool,
    pub tool_count: usize,
    pub resource_count: usize,
    pub template_count: usize,
    /// Connect failure message when `connected` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionListScope {
    CurrentFolder,
    #[default]
    All,
}

/// How a `session.compact` invocation rewrites history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CompactMode {
    /// Summarize the dropped prefix with the model and keep the recent tail.
    #[default]
    Summarize,
    /// Start a fresh window without calling the model; keep the latest user
    /// message. hostd stays authoritative for the rewrite.
    NewContextWindow,
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
    /// Start an asynchronous OAuth login flow.
    AuthLoginOAuth {
        command_id: CommandId,
        provider: String,
        #[serde(default)]
        mode: OAuthLoginMode,
    },
    /// Cancel the active OAuth login for a provider.
    AuthCancelOAuth {
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
    /// Submit one immutable AgentInput through the canonical work-control path.
    AgentInputSubmit {
        command_id: CommandId,
        input: crate::AgentInput,
    },
    /// Cancel exactly one pending AgentInput by durable input identity.
    AgentInputCancel {
        command_id: CommandId,
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        input_id: crate::AgentInputId,
    },
    /// Interrupt the current work of one AgentInstance without exposing a
    /// short-lived Execution identity. This also covers detached child work
    /// that has no source Turn.
    AgentInterrupt {
        command_id: CommandId,
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
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
    ConfigUpdate {
        command_id: CommandId,
        patch: serde_json::Value,
    },
    /// Request the list of available models from hostd's catalog.
    ModelList {
        command_id: CommandId,
    },
    /// Request the user-visible command catalog from hostd.
    CommandCatalogGet {
        command_id: CommandId,
    },
    /// Page through one agent's durable append-only rollout transcript.
    RolloutPageGet {
        command_id: CommandId,
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_cursor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
    /// Read the exact net workspace diff recorded for one turn.
    TurnDiffGet {
        command_id: CommandId,
        session_id: SessionId,
        turn_id: TurnId,
    },
    /// Manually trigger session compaction (bypasses auto threshold).
    SessionCompact {
        command_id: CommandId,
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        /// How to rewrite history. Omitted by older clients → `Summarize`.
        #[serde(default)]
        mode: CompactMode,
    },
    /// List currently running external processes (the `process` tool's live
    /// set). Mirrors codex-rs `backgroundTerminals/list`.
    ProcessList {
        command_id: CommandId,
    },
    /// Terminate a running external process by its `process` tool id
    /// (group SIGTERM → SIGKILL). Mirrors codex-rs
    /// `backgroundTerminals/terminate`.
    ProcessStop {
        command_id: CommandId,
        process_id: String,
    },
    /// List MCP server connection status (F-13): one entry per configured
    /// server with tool/resource/template counts and connect errors.
    McpStatus {
        command_id: CommandId,
    },
    /// Get settings under a namespace (e.g. "tui").
    ConfigGet {
        command_id: CommandId,
        namespace: String,
    },
    /// Query all active agents.
    /// List available named agents (System/Workspace configurations)
    AgentSpecList {
        command_id: CommandId,
    },
    /// List active running agents for a session
    AgentList {
        command_id: CommandId,
        session_id: SessionId,
    },
    /// Subscribe to a concrete AgentInstance view.
    AgentSubscribe {
        command_id: CommandId,
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_seq: Option<u64>,
    },
    /// Unsubscribe from a concrete AgentInstance view.
    AgentUnsubscribe {
        command_id: CommandId,
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
    },
}

impl Command {
    pub fn command_id(&self) -> &str {
        match self {
            Self::AuthSetApiKey { command_id, .. }
            | Self::AuthLoginOAuth { command_id, .. }
            | Self::AuthCancelOAuth { command_id, .. }
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
            | Self::AgentInputSubmit { command_id, .. }
            | Self::AgentInputCancel { command_id, .. }
            | Self::AgentInterrupt { command_id, .. }
            | Self::ApprovalRespond { command_id, .. }
            | Self::UserInteractionRespond { command_id, .. }
            | Self::StateSnapshot { command_id, .. }
            | Self::ConfigUpdate { command_id, .. }
            | Self::ModelList { command_id }
            | Self::CommandCatalogGet { command_id }
            | Self::RolloutPageGet { command_id, .. }
            | Self::TurnDiffGet { command_id, .. }
            | Self::SessionCompact { command_id, .. }
            | Self::ProcessList { command_id }
            | Self::ProcessStop { command_id, .. }
            | Self::McpStatus { command_id }
            | Self::ConfigGet { command_id, .. }
            | Self::AgentSpecList { command_id, .. }
            | Self::AgentList { command_id, .. }
            | Self::AgentSubscribe { command_id, .. }
            | Self::AgentUnsubscribe { command_id, .. } => command_id,
        }
    }

    pub fn submit_follow_up(
        command_id: impl AsRef<str>,
        session_id: impl AsRef<str>,
        agent_instance_id: impl AsRef<str>,
        content: crate::MessageContent,
    ) -> Self {
        let command_id = command_id.as_ref().to_string();
        Self::AgentInputSubmit {
            input: crate::AgentInput::user(
                command_id.clone(),
                session_id.as_ref(),
                agent_instance_id.as_ref(),
                crate::AgentInputDelivery::FollowUp,
                content,
            ),
            command_id,
        }
    }

    pub fn submit_steer(
        command_id: impl AsRef<str>,
        session_id: impl AsRef<str>,
        agent_instance_id: impl AsRef<str>,
        content: crate::MessageContent,
    ) -> Self {
        let command_id = command_id.as_ref().to_string();
        Self::AgentInputSubmit {
            input: crate::AgentInput::user(
                command_id.clone(),
                session_id.as_ref(),
                agent_instance_id.as_ref(),
                crate::AgentInputDelivery::SteerActive,
                content,
            ),
            command_id,
        }
    }
}

#[cfg(test)]
#[path = "command/tests.rs"]
mod tests;

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
    #[error("session observation failed: {0}")]
    ObservationFailed(String),
}
