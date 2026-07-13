//! Serializable contracts for the unified agent runtime API.
//!
//! Runtime traits and side-effecting ports intentionally live in `orchd`; this
//! module contains only values that cross the host/orchestrator boundary.
//!
//! Agent run completion is a command result, not a Session observation event.
//! Task/Work snapshot types remain for session subscribe snapshots and legacy
//! shard projection; they are not the product control surface.

use serde::{Deserialize, Serialize};

use crate::{Message, MessageRole};

pub type RequestId = String;
pub type TaskId = String;
pub type WorkId = String;
pub type MessageId = String;
pub type ExecutionId = String;

/// Committed transcript fragment used when resuming a root execution shard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentResumeState {
    pub transcript: Vec<Message>,
    pub head_message_id: Option<MessageId>,
    pub transcript_seq: u64,
    pub committed_message_ids: Vec<MessageId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WorkStatus {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkSnapshot {
    pub work_id: WorkId,
    pub status: WorkStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub session_id: String,
    pub task_id: TaskId,
    pub agent_id: String,
    pub parent_task_id: Option<TaskId>,
    pub status: crate::execution::ExecutionStatus,
    pub active_work: Option<WorkSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionCursor {
    pub epoch: String,
    pub seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRuntimeSnapshot {
    pub session_id: String,
    pub root_agent_instance_id: Option<crate::AgentInstanceId>,
    pub active_agent_instance_id: Option<crate::AgentInstanceId>,
    pub tasks: Vec<TaskSnapshot>,
    pub cursor: SessionCursor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeRequest {
    pub session_id: String,
    pub task_id: Option<TaskId>,
    pub after: Option<SessionCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutputEnvelope {
    pub session_id: String,
    pub emitted_at: i64,
    pub output: SessionOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "lane", content = "value", rename_all = "camelCase")]
pub enum SessionOutput {
    Event(SessionEventEnvelope),
    Delta(RealtimeDeltaEnvelope),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionEventEnvelope {
    pub agent_instance_id: crate::AgentInstanceId,
    pub agent_id: String,
    pub transcript_seq: u64,
    pub cursor: SessionCursor,
    pub event: SessionEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionEvent {
    MessageCommitted {
        message_id: MessageId,
        work_id: WorkId,
        role: MessageRole,
    },
    ToolCommitted {
        message_id: MessageId,
        work_id: WorkId,
        tool_call_id: String,
    },
    InteractionRequested {
        request: serde_json::Value,
    },
    InteractionResolved {
        resolution: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeDeltaEnvelope {
    pub agent_instance_id: crate::AgentInstanceId,
    pub execution_id: ExecutionId,
    pub agent_id: String,
    pub work_id: WorkId,
    pub message_id: Option<MessageId>,
    pub delta_seq: u64,
    pub delta: RealtimeDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RealtimeDelta {
    MessageStarted {
        role: MessageRole,
    },
    Text {
        content_index: u32,
        delta: String,
    },
    Thinking {
        content_index: u32,
        delta: String,
    },
    ToolCall {
        content_index: u32,
        tool_call_id: String,
        delta: String,
    },
    MessageEnded {
        stop_reason: Option<String>,
        error_message: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reliable_product_event_contains_no_execution_identity() {
        let value = serde_json::to_value(SessionEventEnvelope {
            agent_instance_id: "root".into(),
            agent_id: "main".into(),
            transcript_seq: 1,
            cursor: SessionCursor {
                epoch: "epoch".into(),
                seq: 1,
            },
            event: SessionEvent::MessageCommitted {
                message_id: "message-1".into(),
                work_id: "turn-1".into(),
                role: MessageRole::Assistant,
            },
        })
        .unwrap();
        let serialized = serde_json::to_string(&value).unwrap();
        assert!(!serialized.contains("executionId"));
        assert!(!serialized.contains("execution_id"));
    }
}
