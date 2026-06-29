use crate::messages::Message;
use crate::model::ProviderInfo;
use crate::session::SessionTreeEntry;
use serde::{Deserialize, Serialize};

pub type SessionId = String;
pub type TurnId = String;
pub type MessageId = String;
pub type ToolCallId = String;
pub type ApprovalId = String;
pub type TaskId = String;
pub type AgentId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    AuthLoginDeviceCode {
        provider: String,
        user_code: String,
        verification_uri: String,
    },
    AuthLoginSuccess {
        provider: String,
    },
    AuthLoginFailed {
        provider: String,
        error: String,
    },
    /// Auth credentials removed successfully.
    AuthLoggedOut {
        provider: String,
    },
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
        message: Message,
    },
    ToolResultCommitted {
        session_id: SessionId,
        message_id: MessageId,
        task_id: TaskId,
        agent_id: AgentId,
        message: Message,
    },
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
    SessionCreated {
        session_id: SessionId,
        cwd: String,
        timestamp: i64,
    },
    SessionOpened {
        session_id: SessionId,
        snapshot: SessionSnapshot,
        timestamp: i64,
    },
    SessionListed {
        sessions: Vec<SessionSummary>,
        timestamp: i64,
    },
    ModelListed {
        providers: Vec<ProviderInfo>,
        timestamp: i64,
    },
    StateSnapshot {
        session_id: SessionId,
        snapshot: SessionSnapshot,
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
        #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingLevel")]
        thinking_level: Option<crate::model::ThinkingLevel>,
        timestamp: i64,
    },
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

impl Event {
    pub fn is_domain(&self) -> bool {
        matches!(
            self,
            Event::UserMessageSubmitted { .. }
                | Event::AssistantMessageCompleted { .. }
                | Event::ToolResultCommitted { .. }
                | Event::TurnStarted { .. }
                | Event::TurnCompleted { .. }
                | Event::TurnFailed { .. }
                | Event::TurnCancelled { .. }
                | Event::TaskCreated { .. }
                | Event::TaskStarted { .. }
                | Event::TaskCompleted { .. }
                | Event::TaskFailed { .. }
                | Event::TaskCancelled { .. }
                | Event::TaskJoined { .. }
                | Event::TaskSteered { .. }
                | Event::SessionCreated { .. }
                | Event::SessionOpened { .. }
                | Event::SessionListed { .. }
                | Event::ModelListed { .. }
                | Event::StateSnapshot { .. }
                | Event::QueueUpdate { .. }
                | Event::ModelConfigChanged { .. }
        )
    }

    pub fn is_streaming(&self) -> bool {
        !self.is_domain()
    }
}

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
    Tool,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Accept,
    Decline,
    AcceptSession,
    AcceptWorkspace,
    AcceptPermanent,
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
pub struct SessionSnapshot {
    pub session_id: SessionId,
    pub cwd: String,
    pub seq: u64,
    pub entries: Vec<SessionTreeEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_leaf_id: Option<String>,
    pub active_turn: Option<TurnSnapshot>,
    pub pending_approvals: Vec<ApprovalSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Cumulative token usage and cost across all turns
    #[serde(skip_serializing_if = "Option::is_none", rename = "cumulativeUsage")]
    pub cumulative_usage: Option<crate::messages::Usage>,
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
