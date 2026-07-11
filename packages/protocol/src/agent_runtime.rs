//! Serializable contracts for the unified agent runtime API.
//!
//! Runtime traits and side-effecting ports intentionally live in `orchd`; this
//! module contains only values that cross the host/orchestrator boundary.

use serde::{Deserialize, Serialize};

use crate::{HostTaskContext, Message, MessageContent, MessageRole};

pub type RequestId = String;
pub type TaskId = String;
pub type WorkId = String;
pub type MessageId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskMode {
    Attached,
    Detached,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InputSource {
    User,
    Task { task_id: TaskId, agent_id: String },
    System { component: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InputDelivery {
    Immediate,
    AfterCurrentStep,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub request_id: RequestId,
    pub session_id: String,
    pub task_id: Option<TaskId>,
    pub agent_id: String,
    pub parent_task_id: Option<TaskId>,
    pub source: InputSource,
    pub mode: TaskMode,
    pub host_context: HostTaskContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_history: Option<Vec<Message>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTaskInput {
    pub request_id: RequestId,
    pub session_id: String,
    pub task_id: TaskId,
    pub message_id: MessageId,
    pub work_id: WorkId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
    pub source: InputSource,
    pub content: MessageContent,
    pub delivery: InputDelivery,
    pub submitted_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InputDisposition {
    Accepted,
    Queued,
    Duplicate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InputReceipt {
    pub request_id: RequestId,
    pub task_id: TaskId,
    pub work_id: WorkId,
    pub message_id: MessageId,
    pub disposition: InputDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Created,
    Idle,
    Running,
    Failed,
    Closed,
    Terminated,
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
pub struct TaskHandle {
    pub session_id: String,
    pub task_id: TaskId,
    pub agent_id: String,
    pub status: TaskStatus,
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
    pub status: TaskStatus,
    pub active_work: Option<WorkSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TaskControlRequest {
    Close {
        request_id: RequestId,
        task_id: TaskId,
    },
    Reopen {
        request_id: RequestId,
        task_id: TaskId,
    },
    CancelWork {
        request_id: RequestId,
        task_id: TaskId,
        work_id: WorkId,
    },
    Terminate {
        request_id: RequestId,
        task_id: TaskId,
    },
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
    pub root_task_id: Option<TaskId>,
    pub active_task_id: Option<TaskId>,
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
    pub task_id: TaskId,
    pub agent_id: String,
    pub task_seq: u64,
    pub cursor: SessionCursor,
    pub event: SessionEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionEvent {
    TaskChanged {
        snapshot: TaskSnapshot,
    },
    WorkChanged {
        snapshot: WorkSnapshot,
    },
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
    pub task_id: TaskId,
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
    fn create_task_does_not_contain_prompt() {
        let fields = serde_json::to_value(CreateTaskRequest {
            request_id: "req-1".into(),
            session_id: "session-1".into(),
            task_id: None,
            agent_id: "coder".into(),
            parent_task_id: None,
            source: InputSource::User,
            mode: TaskMode::Attached,
            host_context: HostTaskContext::new("session-1"),
            initial_history: None,
        })
        .unwrap();
        assert!(fields.get("prompt").is_none());
    }
}
