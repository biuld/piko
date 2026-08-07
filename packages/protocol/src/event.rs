use crate::HostCommandDescriptor;
use crate::model::ProviderInfo;
use crate::session::SessionTreeEntry;

use serde::{Deserialize, Serialize};

use crate::AgentStatus;

pub type SessionId = String;
pub type TurnId = String;
pub type MessageId = String;
pub type ToolCallId = String;
pub type ApprovalId = String;
pub type InteractionId = String;
pub type InteractionQuestionId = String;
pub type InteractionChoiceId = String;
pub type AgentId = String;

/// Agent 状态信息，由 hostd 维护，TUI 通过 AgentList 查询
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
    /// 带可靠事件边界的 session hydration/reconciliation。
    SessionReconciled(SessionReconciledEvent),
    /// Authoritative transition from a visible session to no session.
    SessionCleared(SessionClearedEvent),
    /// 用户交互生命周期；不属于消息 realtime delta。
    Interaction(InteractionEvent),
    /// 完整 agent 投影，以 agent_instance_id / execution_id 为实体 identity。
    AgentChanged(AgentInfo),
    TurnLifecycle(TurnEvent),
    AgentRunLifecycle(AgentRunEvent),
    TurnDiff(TurnDiffEvent),
    Approval(ApprovalEvent),
    Queue(QueueEvent),
    Model(ModelEvent),
    /// Host-authoritative context fill / cost chrome (F-22 / D-34).
    Usage(UsageEvent),
    /// Unified stream-item patch envelope — sole live stream transport (F-22).
    StreamItem(crate::StreamItemPatch),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnDiffEvent {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub files: Vec<TurnFileChange>,
    pub unified_diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnFileChange {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCommittedEvent {
    pub session_id: SessionId,
    pub agent_instance_id: crate::AgentInstanceId,
    pub agent_id: AgentId,
    /// Interaction Turn this message was committed under, if any.
    pub source_turn_id: String,
    pub message_id: MessageId,
    pub transcript_seq: u64,
    pub message: crate::messages::Message,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionClearedEvent {
    pub previous_session_id: SessionId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolExecutionEvent {
    Started {
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        tool_call_id: ToolCallId,
        tool_name: String,
        args: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
    },
    Ended {
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
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
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
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
        session_id: SessionId,
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

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandResult {
    Empty,
    SessionCreated {
        session_id: SessionId,
        cwd: String,
        timestamp: i64,
    },
    /// Session identity only — visible view arrives via `SessionReconciled`.
    SessionOpened {
        session_id: SessionId,
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
    ModelListed {
        providers: Vec<ProviderInfo>,
        timestamp: i64,
    },
    CommandCatalogListed {
        commands: Vec<HostCommandDescriptor>,
        timestamp: i64,
    },
    PromptDebugged {
        snapshot: crate::PromptDebugSnapshot,
        timestamp: i64,
    },
    RolloutPaged {
        page: crate::RolloutPage,
        timestamp: i64,
    },
    TurnDiffGot {
        #[serde(skip_serializing_if = "Option::is_none")]
        diff: Option<TurnDiffEvent>,
        timestamp: i64,
    },
    /// Live set of external processes spawned by the `process` tool (F-08).
    ProcessListed {
        processes: Vec<crate::command::ProcessInfo>,
        timestamp: i64,
    },
    /// Result of terminating one process (F-08 client surface).
    ProcessStopped {
        process_id: String,
        /// False when no such process was running.
        stopped: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        signal: Option<i32>,
        timestamp: i64,
    },
    /// MCP server connection status (F-13): one entry per configured server.
    McpStatusListed {
        servers: Vec<crate::command::McpServerInfo>,
        timestamp: i64,
    },
    AgentSpecListed {
        agents: Vec<crate::agents::AgentSpec>,
        timestamp: i64,
    },
    AgentListed {
        session_id: SessionId,
        agents: Vec<AgentInfo>,
        timestamp: i64,
    },
    AgentSubscribed {
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnEvent {
    Queued {
        session_id: SessionId,
        turn_id: TurnId,
        agent_instance_id: crate::AgentInstanceId,
        timestamp: i64,
    },
    Started {
        session_id: SessionId,
        turn_id: TurnId,
        agent_instance_id: crate::AgentInstanceId,
        timestamp: i64,
    },
    Completed {
        session_id: SessionId,
        turn_id: TurnId,
        agent_instance_id: crate::AgentInstanceId,
        /// Rolled-up model-step usage for this turn (hostd ledger; F-15/D-29).
        #[serde(default)]
        usage: crate::messages::Usage,
        timestamp: i64,
    },
    Failed {
        session_id: SessionId,
        turn_id: TurnId,
        agent_instance_id: crate::AgentInstanceId,
        error: String,
        /// Partial turn usage when steps completed before failure (F-15/D-29).
        #[serde(default)]
        usage: crate::messages::Usage,
        timestamp: i64,
    },
    Cancelled {
        session_id: SessionId,
        turn_id: TurnId,
        agent_instance_id: crate::AgentInstanceId,
        /// Partial turn usage when steps completed before cancel (F-15/D-29).
        #[serde(default)]
        usage: crate::messages::Usage,
        timestamp: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRunEvent {
    Started {
        session_id: SessionId,
        run_id: String,
        agent_instance_id: crate::AgentInstanceId,
        timestamp: i64,
    },
    Completed {
        session_id: SessionId,
        run_id: String,
        agent_instance_id: crate::AgentInstanceId,
        timestamp: i64,
    },
    Failed {
        session_id: SessionId,
        run_id: String,
        agent_instance_id: crate::AgentInstanceId,
        error: String,
        timestamp: i64,
    },
}

/// lifecycle channel — hostd Turn lifecycle (Execution observation is separate).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "lc_kind", content = "event", rename_all = "snake_case")]
pub enum LifecycleEvent {
    Turn(TurnEvent),
    AgentRun(AgentRunEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalEvent {
    Requested {
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        approval_id: ApprovalId,
        tool_name: String,
        tool_args: serde_json::Value,
        /// F-13: operator-authored approval prompt (MCP approval templates);
        /// absent → clients keep the generic question.
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    Resolved {
        session_id: SessionId,
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
        /// Active model context window (tokens) from the host catalog when known.
        /// Clients use this for status chrome (`used/size`) without relying on a
        /// local catalog cache (F-22 / D-34).
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "contextWindow"
        )]
        context_window: Option<u64>,
        timestamp: i64,
    },
}

impl From<ModelEvent> for ServerMessage {
    fn from(event: ModelEvent) -> Self {
        Self::Model(event)
    }
}

/// Live usage / context chrome projection (ACP-inspired `usage_update`, piko-native).
///
/// Prefer this for status bars over client-side roll-up of turn usage alone.
/// Host typically emits `Updated` immediately after a terminal `TurnLifecycle`
/// message; clients may treat `cumulative` as authoritative session ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UsageEvent {
    Updated {
        session_id: SessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_instance_id: Option<crate::AgentInstanceId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_id: Option<TurnId>,
        /// Context fill estimate (prompt side: `input + cache_read`).
        used: u64,
        /// Active model context window when host can resolve it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<u64>,
        /// Session cumulative ledger after this update (when known).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cumulative: Option<crate::messages::Usage>,
        /// Turn-scoped usage that triggered this projection (when applicable).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_usage: Option<crate::messages::Usage>,
        timestamp: i64,
    },
}

impl From<UsageEvent> for ServerMessage {
    fn from(event: UsageEvent) -> Self {
        Self::Usage(event)
    }
}

impl From<crate::StreamItemPatch> for ServerMessage {
    fn from(event: crate::StreamItemPatch) -> Self {
        Self::StreamItem(event)
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
    Context,
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
    /// No decision arrived before the approval deadline; the request expired
    /// and the tool call fails closed.
    Expired,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_leaf_id: Option<String>,
    /// Authoritative AgentInstance view selected for this Session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_agent_instance_id: Option<crate::AgentInstanceId>,
    #[serde(default)]
    pub active_turns: Vec<TurnSnapshot>,
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
    pub agent_instance_id: crate::AgentInstanceId,
    pub status: TurnStatus,
    pub assistant_text: String,
    pub tool_calls: Vec<ToolCallSnapshot>,
    /// In-flight or last known roll-up for this turn (hostd ledger; F-15/D-29).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::messages::Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Queued,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalSnapshot {
    pub approval_id: ApprovalId,
    pub agent_instance_id: crate::AgentInstanceId,
    pub tool_name: String,
    /// Tool arguments (or structured request payload).
    pub request: serde_json::Value,
    /// F-13: operator-authored approval prompt (MCP approval templates),
    /// rendered with placeholders substituted. Absent → clients keep the
    /// generic question.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    pub status: ApprovalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    /// The request expired before a user decision arrived.
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInteractionSnapshot {
    pub interaction_id: InteractionId,
    pub agent_instance_id: crate::AgentInstanceId,
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
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        source_turn_id: String,
        message: crate::messages::Message,
    },
    /// Assistant 消息完成
    Finalized {
        session_id: SessionId,
        message_id: MessageId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        source_turn_id: String,
        message: crate::messages::Message,
    },
    /// 工具调用提交
    ToolCallCommitted {
        session_id: SessionId,
        message_id: MessageId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        source_turn_id: String,
        parent_message_id: MessageId,
        message: crate::messages::Message,
    },
    /// 工具执行结果
    ToolResultCommitted {
        session_id: SessionId,
        message_id: MessageId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        source_turn_id: String,
        message: crate::messages::Message,
    },
}

#[cfg(test)]
mod observation_projection_tests {
    use super::*;

    #[test]
    fn committed_and_stream_server_messages_round_trip() {
        let committed = ServerMessage::TranscriptCommitted(TranscriptCommittedEvent {
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            source_turn_id: "turn-1".into(),
            message_id: "message-1".into(),
            transcript_seq: 3,
            message: crate::Message::User {
                content: crate::MessageContent::String("hello".into()),
                timestamp: Some(1),
            },
        });
        let stream = ServerMessage::StreamItem(crate::StreamItemPatch {
            session_id: Some("session-1".into()),
            agent_instance_id: Some("root".into()),
            item_id: "message-2".into(),
            item_kind: crate::StreamItemKind::AgentMessage,
            op: crate::StreamItemOp::AppendChunk,
            text: Some("world".into()),
            content_index: Some(0),
            delta_seq: Some(4),
            fields: Some(serde_json::json!({"parentMessageId": "message-2"})),
        });

        for event in [committed, stream] {
            let json = serde_json::to_string(&event).unwrap();
            let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                serde_json::to_value(event).unwrap()
            );
        }
    }

    #[test]
    fn usage_updated_round_trips() {
        let usage = ServerMessage::Usage(UsageEvent::Updated {
            session_id: "session-1".into(),
            agent_instance_id: Some("root".into()),
            turn_id: Some("turn-1".into()),
            used: 13_000,
            size: Some(128_000),
            cumulative: Some(crate::messages::Usage {
                input: 10_000,
                output: 100,
                cache_read: 3_000,
                cache_write: 0,
                total_tokens: 13_100,
                cost: Default::default(),
            }),
            turn_usage: Some(crate::messages::Usage {
                input: 10_000,
                output: 100,
                cache_read: 3_000,
                cache_write: 0,
                total_tokens: 13_100,
                cost: Default::default(),
            }),
            timestamp: 42,
        });
        let json = serde_json::to_string(&usage).unwrap();
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(usage).unwrap()
        );
    }

    #[test]
    fn stream_item_round_trips() {
        let patch = crate::StreamItemPatch {
            session_id: Some("session-1".into()),
            agent_instance_id: Some("root".into()),
            item_id: "msg-1".into(),
            item_kind: crate::StreamItemKind::AgentMessage,
            op: crate::StreamItemOp::AppendChunk,
            text: Some("hi".into()),
            content_index: Some(0),
            delta_seq: Some(2),
            fields: None,
        };
        let event = ServerMessage::StreamItem(patch);
        let json = serde_json::to_string(&event).unwrap();
        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(event).unwrap()
        );
    }
}
