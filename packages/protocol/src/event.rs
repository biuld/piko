use crate::CommandCatalogItem;
use crate::model::ProviderInfo;
use crate::session::SessionTreeEntry;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::AgentStatus;
use crate::agent_runtime::RealtimeDelta;

pub type SessionId = String;
pub type TurnId = String;
pub type MessageId = String;
pub type ToolCallId = String;
pub type ApprovalId = String;
pub type InteractionId = String;
pub type InteractionQuestionId = String;
pub type InteractionChoiceId = String;
pub type TaskId = String;
pub type AgentId = String;

/// Agent 状态信息，由 hostd 维护，TUI 通过 AgentList 查询
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub parent_task_id: Option<TaskId>,
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
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub parent_task_id: Option<TaskId>,
    pub status: Option<AgentStatus>,
    pub next_seq: u64,
    pub events: Vec<SequencedServerMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ServerMessage {
    CommandResponse {
        command_id: crate::CommandId,
        result: Result<CommandResult, String>,
    },
    Auth(AuthEvent),
    /// 已完成 durable commit 的权威 transcript record。
    TranscriptCommitted(TranscriptCommittedEvent),
    /// 可丢的实时消息草稿；不得用于恢复或修改 committed transcript。
    RealtimeMessage(RealtimeMessageEvent),
    /// 带可靠事件边界的 session hydration/reconciliation。
    SessionReconciled(SessionReconciledEvent),
    /// 工具执行过程；与 committed ToolCall/ToolResult transcript 分离。
    ToolExecution(ToolExecutionEvent),
    /// 用户交互生命周期；不属于消息 realtime delta。
    Interaction(InteractionEvent),
    /// 完整 agent 投影，以 task_id / execution_id 为实体 identity。
    AgentChanged(AgentInfo),
    TurnLifecycle(TurnEvent),
    Approval(ApprovalEvent),
    Queue(QueueEvent),
    Model(ModelEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCommittedEvent {
    pub session_id: SessionId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub work_id: String,
    pub message_id: MessageId,
    pub task_seq: u64,
    pub message: crate::messages::Message,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeMessageEvent {
    pub session_id: SessionId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub message_id: MessageId,
    pub delta_seq: u64,
    pub delta: RealtimeDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileReason {
    InitialHydration,
    ExplicitRefresh,
    Reconnect,
    RetentionExhausted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReconciledEvent {
    pub session_id: SessionId,
    pub reason: ReconcileReason,
    pub cursor: crate::agent_runtime::SessionCursor,
    pub snapshot: SessionSnapshot,
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolExecutionEvent {
    Started {
        task_id: TaskId,
        agent_id: AgentId,
        tool_call_id: ToolCallId,
        tool_name: String,
        args: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
    },
    Ended {
        task_id: TaskId,
        agent_id: AgentId,
        tool_call_id: ToolCallId,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionEvent {
    Requested {
        task_id: TaskId,
        agent_id: AgentId,
        interaction_id: InteractionId,
        tool_call_id: ToolCallId,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        questions: Vec<InteractionQuestion>,
        require_confirm: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        auto_resolution_ms: Option<u64>,
    },
    Resolved {
        task_id: TaskId,
        agent_id: AgentId,
        interaction_id: InteractionId,
        status: UserInteractionStatus,
    },
}

impl ServerMessage {
    pub fn command_id(&self) -> Option<&str> {
        match self {
            Self::CommandResponse { command_id, .. } => Some(command_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandResult {
    Empty,
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
    SessionNavigated {
        session_id: SessionId,
        old_leaf_id: Option<String>,
        new_leaf_id: Option<String>,
        selected_entry_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        editor_text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary_entry: Option<SessionTreeEntry>,
        timestamp: i64,
    },
    StateSnapshot {
        session_id: SessionId,
        snapshot: SessionSnapshot,
        timestamp: i64,
    },
    ModelListed {
        providers: Vec<ProviderInfo>,
        timestamp: i64,
    },
    CommandCatalogListed {
        commands: Vec<CommandCatalogItem>,
        timestamp: i64,
    },
    AgentSpecListed {
        agents: Vec<crate::agents::AgentSpec>,
        timestamp: i64,
    },
    AgentListed {
        agents: Vec<AgentInfo>,
        timestamp: i64,
    },
    AgentSubscribed {
        task_id: TaskId,
        agent_id: AgentId,
        snapshot: AgentViewSnapshot,
        replay: Vec<SequencedServerMessage>,
        next_seq: u64,
    },
    ConfigEntry {
        namespace: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthEvent {
    LoginDeviceCode {
        provider: String,
        user_code: String,
        verification_uri: String,
    },
    LoginSuccess {
        provider: String,
    },
    LoginFailed {
        provider: String,
        error: String,
    },
    LoggedOut {
        provider: String,
    },
}

impl From<AuthEvent> for ServerMessage {
    fn from(event: AuthEvent) -> Self {
        Self::Auth(event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnEvent {
    Started {
        session_id: SessionId,
        turn_id: TurnId,
        root_task_id: TaskId,
        timestamp: i64,
    },
    Completed {
        session_id: SessionId,
        turn_id: TurnId,
        total_tasks: u32,
        timestamp: i64,
    },
    Failed {
        session_id: SessionId,
        turn_id: TurnId,
        error: String,
        timestamp: i64,
    },
    Cancelled {
        session_id: SessionId,
        turn_id: TurnId,
        timestamp: i64,
    },
}

/// lifecycle channel — hostd Turn lifecycle (Execution observation is separate).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "lc_kind", content = "event", rename_all = "snake_case")]
pub enum LifecycleEvent {
    Turn(TurnEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEvent {
    Created {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        parent_task_id: Option<TaskId>,
        source_agent_id: Option<AgentId>,
        prompt: String,
        work_id: TurnId,
        timestamp: i64,
    },
    Started {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        timestamp: i64,
    },
    Idle {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        total_steps: u32,
        summary: String,
        timestamp: i64,
    },
    Completed {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        total_steps: u32,
        summary: String,
        final_status: String,
        timestamp: i64,
    },
    Failed {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        error: String,
        timestamp: i64,
    },
    Cancelled {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        timestamp: i64,
    },
    Closed {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        timestamp: i64,
    },
    Reopened {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        timestamp: i64,
    },
    Joined {
        session_id: SessionId,
        task_id: TaskId,
        parent_task_id: TaskId,
        result: serde_json::Value,
        timestamp: i64,
    },
    Steered {
        session_id: SessionId,
        task_id: TaskId,
        source_task_id: TaskId,
        source_agent_id: AgentId,
        message: String,
        timestamp: i64,
    },
}

impl TaskEvent {
    pub fn task_id(&self) -> &str {
        match self {
            Self::Created { task_id, .. }
            | Self::Started { task_id, .. }
            | Self::Idle { task_id, .. }
            | Self::Completed { task_id, .. }
            | Self::Failed { task_id, .. }
            | Self::Cancelled { task_id, .. }
            | Self::Closed { task_id, .. }
            | Self::Reopened { task_id, .. }
            | Self::Joined { task_id, .. }
            | Self::Steered { task_id, .. } => task_id,
        }
    }
}

/// Work lifecycle events scoped to a single input-driven execution cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkEvent {
    Started {
        session_id: SessionId,
        task_id: TaskId,
        work_id: String,
        timestamp: i64,
    },
    Succeeded {
        session_id: SessionId,
        task_id: TaskId,
        work_id: String,
        timestamp: i64,
    },
    Failed {
        session_id: SessionId,
        task_id: TaskId,
        work_id: String,
        error: String,
        timestamp: i64,
    },
    Cancelled {
        session_id: SessionId,
        task_id: TaskId,
        work_id: String,
        timestamp: i64,
    },
}

impl WorkEvent {
    pub fn work_id(&self) -> &str {
        match self {
            Self::Started { work_id, .. }
            | Self::Succeeded { work_id, .. }
            | Self::Failed { work_id, .. }
            | Self::Cancelled { work_id, .. } => work_id,
        }
    }

    pub fn task_id(&self) -> &str {
        match self {
            Self::Started { task_id, .. }
            | Self::Succeeded { task_id, .. }
            | Self::Failed { task_id, .. }
            | Self::Cancelled { task_id, .. } => task_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalEvent {
    Requested {
        task_id: TaskId,
        agent_id: AgentId,
        approval_id: ApprovalId,
        tool_name: String,
        tool_args: serde_json::Value,
    },
    Resolved {
        task_id: TaskId,
        agent_id: AgentId,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    },
}

impl From<ApprovalEvent> for ServerMessage {
    fn from(event: ApprovalEvent) -> Self {
        Self::Approval(event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueueEvent {
    Updated {
        session_id: SessionId,
        steer_count: u32,
        follow_up_count: u32,
        next_turn_count: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        steer_preview: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        follow_up_preview: Option<String>,
    },
}

impl From<QueueEvent> for ServerMessage {
    fn from(event: QueueEvent) -> Self {
        Self::Queue(event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelEvent {
    ConfigChanged {
        model_id: String,
        provider: String,
        #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingLevel")]
        thinking_level: Option<crate::model::ThinkingLevel>,
        timestamp: i64,
    },
}

impl From<ModelEvent> for ServerMessage {
    fn from(event: ModelEvent) -> Self {
        Self::Model(event)
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InteractionQuestion {
    pub id: InteractionQuestionId,
    pub header: String,
    pub prompt: String,
    pub choices: Vec<InteractionChoice>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InteractionChoice {
    pub id: InteractionChoiceId,
    pub label: String,
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InteractionInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InteractionInput {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InteractionAnswer {
    pub question_id: InteractionQuestionId,
    pub choice_id: InteractionChoiceId,
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserInteractionResponse {
    Submit {
        answers: Vec<InteractionAnswer>,
    },
    Cancel {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UserInteractionStatus {
    Pending,
    Submitted,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub cwd: String,
    pub seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_message: Option<String>,
    #[serde(default)]
    pub message_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub session_id: SessionId,
    pub cwd: String,
    pub seq: u64,
    pub entries: Vec<SessionTreeEntry>,
    #[serde(default)]
    pub tasks: HashMap<TaskId, crate::agents::AgentTaskState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_leaf_id: Option<String>,
    pub active_turn: Option<TurnSnapshot>,
    pub pending_approvals: Vec<ApprovalSnapshot>,
    #[serde(default)]
    pub pending_interactions: Vec<UserInteractionSnapshot>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInteractionSnapshot {
    pub interaction_id: InteractionId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub tool_call_id: ToolCallId,
    pub status: UserInteractionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub questions: Vec<InteractionQuestion>,
    pub require_confirm: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_resolution_ms: Option<u64>,
}

// ── Dispatch framework: typed channel event types ──

/// persist channel — 最终态事件，hostd 消费并转换为 SessionTreeEntry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistEvent {
    /// User-role transcript input, including initial prompts and later steering.
    UserCommitted {
        session_id: SessionId,
        message_id: MessageId,
        task_id: TaskId,
        agent_id: AgentId,
        work_id: String,
        message: crate::messages::Message,
    },
    /// Assistant 消息完成
    Finalized {
        session_id: SessionId,
        message_id: MessageId,
        task_id: TaskId,
        agent_id: AgentId,
        work_id: String,
        message: crate::messages::Message,
    },
    /// 工具调用提交
    ToolCallCommitted {
        session_id: SessionId,
        message_id: MessageId,
        task_id: TaskId,
        agent_id: AgentId,
        work_id: String,
        parent_message_id: MessageId,
        message: crate::messages::Message,
    },
    /// 工具执行结果
    ToolResultCommitted {
        session_id: SessionId,
        message_id: MessageId,
        task_id: TaskId,
        agent_id: AgentId,
        work_id: String,
        message: crate::messages::Message,
    },
}

#[cfg(test)]
mod observation_projection_tests {
    use super::*;

    #[test]
    fn committed_and_realtime_server_messages_round_trip() {
        let committed = ServerMessage::TranscriptCommitted(TranscriptCommittedEvent {
            session_id: "session-1".into(),
            task_id: "task-1".into(),
            agent_id: "main".into(),
            work_id: "work-1".into(),
            message_id: "message-1".into(),
            task_seq: 3,
            message: crate::Message::User {
                content: crate::MessageContent::String("hello".into()),
                timestamp: Some(1),
            },
        });
        let realtime = ServerMessage::RealtimeMessage(RealtimeMessageEvent {
            session_id: "session-1".into(),
            task_id: "task-1".into(),
            agent_id: "main".into(),
            message_id: "message-2".into(),
            delta_seq: 4,
            delta: RealtimeDelta::Text {
                content_index: 0,
                delta: "world".into(),
            },
        });

        for event in [committed, realtime] {
            let json = serde_json::to_string(&event).unwrap();
            let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                serde_json::to_value(event).unwrap()
            );
        }
    }
}
