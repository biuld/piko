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
// Basic ID types
// ============================================================================

pub type CommandId = String;
pub type SessionId = String;
pub type TurnId = String;
pub type MessageId = String;
pub type ToolCallId = String;
pub type ApprovalId = String;
pub type TaskId = String;
pub type AgentId = String;

// ============================================================================
// Commands (TUI → hostd)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
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
    },
}

impl HostCommand {
    pub fn command_id(&self) -> &str {
        match self {
            Self::SessionCreate { command_id, .. }
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
            | Self::ConfigSet { command_id, .. } => command_id,
        }
    }
}

// ============================================================================
// Command acks (not domain events)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandAck {
    CommandAccepted { command_id: CommandId },
    CommandRejected { command_id: CommandId, reason: String },
}

impl CommandAck {
    pub fn command_id(&self) -> &str {
        match self {
            Self::CommandAccepted { command_id } | Self::CommandRejected { command_id, .. } => command_id,
        }
    }
}

// ============================================================================
// Unified HostEvent — 21 variants
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostEvent {
    // ═══ Domain: Messages (3) ═══
    UserMessageSubmitted {
        session_id: SessionId,
        message_id: MessageId,
        task_id: TaskId,
        text: String,
        timestamp: i64,
    },
    AssistantMessageCompleted {
        session_id: SessionId,
        message_id: MessageId,
        task_id: TaskId,
        agent_id: AgentId,
        text: String,
        tool_calls: Vec<ToolCallRef>,
        model: String,
        provider: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
        timestamp: i64,
    },
    ToolResultCommitted {
        session_id: SessionId,
        message_id: MessageId,
        task_id: TaskId,
        agent_id: AgentId,
        tool_call_id: ToolCallId,
        tool_name: String,
        content: serde_json::Value,
        is_error: bool,
        timestamp: i64,
    },

    // ═══ Domain: Turn (4) ═══
    TurnStarted {
        session_id: SessionId,
        turn_id: TurnId,
        root_task_id: TaskId,
        timestamp: i64,
    },
    TurnCompleted {
        session_id: SessionId,
        turn_id: TurnId,
        total_tasks: u32,
        timestamp: i64,
    },
    TurnFailed {
        session_id: SessionId,
        turn_id: TurnId,
        error: String,
        timestamp: i64,
    },
    TurnCancelled {
        session_id: SessionId,
        turn_id: TurnId,
        timestamp: i64,
    },

    // ═══ Domain: Task (8) ═══
    TaskCreated {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        parent_task_id: Option<TaskId>,
        source_agent_id: Option<AgentId>,
        prompt: String,
        turn_id: TurnId,
        timestamp: i64,
    },
    TaskStarted {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        timestamp: i64,
    },
    TaskCompleted {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        total_steps: u32,
        summary: String,
        final_status: String,
        timestamp: i64,
    },
    TaskFailed {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        error: String,
        timestamp: i64,
    },
    TaskCancelled {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        timestamp: i64,
    },
    TaskTranscriptCommitted {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        parent_task_id: TaskId,
        messages: Vec<serde_json::Value>, // Message objects
        summary: String,
        final_status: String,
        timestamp: i64,
    },
    TaskJoined {
        session_id: SessionId,
        task_id: TaskId,
        parent_task_id: TaskId,
        result: serde_json::Value,
        timestamp: i64,
    },
    TaskSteered {
        session_id: SessionId,
        task_id: TaskId,
        source_task_id: TaskId,
        source_agent_id: AgentId,
        message: String,
        timestamp: i64,
    },

    // ═══ Domain: Session & Config (3) ═══
    SessionCreated {
        session_id: SessionId,
        cwd: String,
        timestamp: i64,
    },
    QueueUpdate {
        session_id: SessionId,
        steer_count: u32,
        follow_up_count: u32,
        next_turn_count: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        steer_preview: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        follow_up_preview: Option<String>,
    },
    ModelConfigChanged {
        session_id: SessionId,
        model_id: String,
        provider: String,
        timestamp: i64,
    },

    // ═══ Streaming: Message (4) ═══
    MessageStart {
        task_id: TaskId,
        agent_id: AgentId,
        message_id: MessageId,
        role: MessageRole,
    },
    MessageEnd {
        task_id: TaskId,
        agent_id: AgentId,
        message_id: MessageId,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason: Option<String>,
    },
    TextDelta {
        task_id: TaskId,
        agent_id: AgentId,
        message_id: MessageId,
        delta: String,
    },
    ThinkingDelta {
        task_id: TaskId,
        agent_id: AgentId,
        message_id: MessageId,
        delta: String,
    },

    // ═══ Streaming: Tool (2) ═══
    ToolStart {
        task_id: TaskId,
        agent_id: AgentId,
        tool_call_id: ToolCallId,
        tool_name: String,
        args: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
    },
    ToolEnd {
        task_id: TaskId,
        agent_id: AgentId,
        tool_call_id: ToolCallId,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    },

    // ═══ Streaming: Approval (2) ═══
    ApprovalRequested {
        task_id: TaskId,
        agent_id: AgentId,
        approval_id: ApprovalId,
        tool_name: String,
        tool_args: serde_json::Value,
    },
    ApprovalResolved {
        task_id: TaskId,
        agent_id: AgentId,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    },
}

impl HostEvent {
    /// Returns true if this is a domain event (persisted to journal).
    pub fn is_domain(&self) -> bool {
        matches!(
            self,
            HostEvent::UserMessageSubmitted { .. }
                | HostEvent::AssistantMessageCompleted { .. }
                | HostEvent::ToolResultCommitted { .. }
                | HostEvent::TurnStarted { .. }
                | HostEvent::TurnCompleted { .. }
                | HostEvent::TurnFailed { .. }
                | HostEvent::TurnCancelled { .. }
                | HostEvent::TaskCreated { .. }
                | HostEvent::TaskStarted { .. }
                | HostEvent::TaskCompleted { .. }
                | HostEvent::TaskFailed { .. }
                | HostEvent::TaskCancelled { .. }
                | HostEvent::TaskTranscriptCommitted { .. }
                | HostEvent::TaskJoined { .. }
                | HostEvent::TaskSteered { .. }
                | HostEvent::SessionCreated { .. }
                | HostEvent::QueueUpdate { .. }
                | HostEvent::ModelConfigChanged { .. }
        )
    }

    /// Returns true if this is a streaming event (real-time only).
    pub fn is_streaming(&self) -> bool {
        !self.is_domain()
    }
}

// ============================================================================
// Companion types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRef {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    Assistant,
    ToolResult,
    User,
    /// Legacy role for tool-call messages (session persistence)
    Tool,
    /// Legacy role for system messages (session persistence)
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Accept,
    Decline,
    AcceptSession,
    AcceptWorkspace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub total_tokens: u64,
    pub cost: UsageCost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

// ============================================================================
// Session-level types (not wire events — used for persistence)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostMessage {
    pub id: MessageId,
    pub role: MessageRole,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub cwd: String,
    pub seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSessionSnapshot {
    pub session_id: SessionId,
    pub cwd: String,
    pub seq: u64,
    pub messages: Vec<HostMessage>,
    pub active_turn: Option<TurnSnapshot>,
    pub pending_approvals: Vec<ApprovalSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnSnapshot {
    pub turn_id: TurnId,
    pub status: TurnStatus,
    pub assistant_text: String,
    pub tool_calls: Vec<ToolCallSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Idle,
    Running,
    WaitingForApproval,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallSnapshot {
    pub tool_call_id: ToolCallId,
    pub name: String,
    pub status: ToolCallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalSnapshot {
    pub approval_id: ApprovalId,
    pub request: serde_json::Value,
    pub status: ApprovalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResumeResponse {
    Events { events: Vec<HostEvent> },
    Snapshot { event: HostEvent },
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum HostProtocolError {
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
