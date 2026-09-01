use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ServerMessage {
    CommandResponse {
        command_id: crate::CommandId,
        result: Result<CommandResult, String>,
    },
    Auth(AuthEvent),
    /// Authoritative transcript record after a durable commit.
    TranscriptCommitted(TranscriptCommittedEvent),
    /// Newly durable non-message session entry with a live Timeline projection.
    /// Message entries remain on `TranscriptCommitted` to keep one authority.
    SessionEntryCommitted(SessionEntryCommittedEvent),
    /// Session hydration/reconciliation with reliable event boundaries.
    SessionReconciled(SessionReconciledEvent),
    /// Authoritative transition from a visible session to no session.
    SessionCleared(SessionClearedEvent),
    /// User interaction lifecycle; not part of the message realtime delta.
    Interaction(InteractionEvent),
    /// Authoritative model-step boundary after its transcript facts are
    /// durable. Message bodies arrive as `TranscriptCommitted` events.
    ModelStepCommitted(crate::agent_work::ModelStepBoundary),
    /// Full agent projection keyed by agent_instance_id as entity identity.
    AgentChanged(AgentInfo),
    AgentWorkDiff(AgentWorkDiffEvent),
    Approval(ApprovalEvent),
    Model(ModelEvent),
    /// Host-authoritative context fill / cost chrome (F-22 / D-34).
    Usage(UsageEvent),
    /// Unified stream-item patch envelope — sole live stream transport (F-22).
    StreamItem(crate::StreamItemPatch),
    /// Host-authoritative current todo list for one agent (F-27).
    TodoListUpdated(crate::TodoListUpdated),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkDiffEvent {
    pub session_id: SessionId,
    pub root_input_id: String,
    pub files: Vec<AgentWorkFileChange>,
    pub unified_diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkFileChange {
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
    pub root_input_id: String,
    pub message_id: MessageId,
    pub transcript_seq: u64,
    pub message: crate::messages::Message,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionEntryCommittedEvent {
    pub session_id: SessionId,
    pub entry: crate::SessionTreeEntry,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root_input_id: Option<String>,
    },
    Ended {
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        tool_call_id: ToolCallId,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<MessageId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root_input_id: Option<String>,
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
