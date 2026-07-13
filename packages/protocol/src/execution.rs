//! Single-agent Execution DTOs.
//!
//! Target model: Session → Turn → Execution → Model Step → Tool.
//! These types are the public contract for the Execution path.

use serde::{Deserialize, Serialize};

use crate::{Message, MessageContent, Usage};

pub type RequestId = String;
pub type SessionId = String;
pub type TurnId = String;
pub type ExecutionId = String;
pub type MessageId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionStatus {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExecutionOutcome {
    Succeeded { usage: Usage },
    Failed { error: String },
    Cancelled { reason: Option<String> },
}

impl ExecutionOutcome {
    pub fn failed(error: impl Into<String>) -> Self {
        Self::Failed {
            error: error.into(),
        }
    }

    pub fn status(&self) -> ExecutionStatus {
        match self {
            Self::Succeeded { .. } => ExecutionStatus::Succeeded,
            Self::Failed { .. } => ExecutionStatus::Failed,
            Self::Cancelled { .. } => ExecutionStatus::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CancelReason {
    UserRequested,
    SessionShutdown,
    RuntimeShutdown,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationContext {
    pub messages: Vec<Message>,
    pub head_message_id: Option<MessageId>,
    pub system_prompt: Option<String>,
}

impl ConversationContext {
    pub fn empty() -> Self {
        Self {
            messages: Vec::new(),
            head_message_id: None,
            system_prompt: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionConfig {
    pub agent_id: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub allow_tool_calls: bool,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            agent_id: "main".into(),
            model: None,
            provider: None,
            allow_tool_calls: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartExecutionRequest {
    pub request_id: RequestId,
    pub session_id: SessionId,
    /// Interaction Turn this Execution is bound to. `None` for child agent
    /// Executions spawned by multi-agent tools (no Interaction Turn).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<TurnId>,
    pub execution_id: ExecutionId,
    pub agent_instance_id: crate::AgentInstanceId,
    pub agent_spec: crate::AgentSpec,
    pub input_message_id: MessageId,
    pub input: MessageContent,
    pub context: ConversationContext,
    pub config: ExecutionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReceipt {
    pub request_id: RequestId,
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<TurnId>,
    pub execution_id: ExecutionId,
    pub agent_instance_id: crate::AgentInstanceId,
    pub status: ExecutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteerExecutionRequest {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub execution_id: ExecutionId,
    pub message_id: MessageId,
    pub content: MessageContent,
    pub submitted_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InputDisposition {
    Accepted,
    Queued,
    Duplicate,
    Overload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionInputReceipt {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub execution_id: ExecutionId,
    pub message_id: MessageId,
    pub disposition: InputDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CancelExecutionRequest {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub execution_id: ExecutionId,
    pub reason: CancelReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CancelReceipt {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub execution_id: ExecutionId,
    /// Actor accepted cancellation intent; terminal outcome is separate.
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSnapshot {
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<TurnId>,
    pub execution_id: ExecutionId,
    pub agent_instance_id: crate::AgentInstanceId,
    pub agent_id: String,
    pub status: ExecutionStatus,
    pub model_step_index: u32,
    pub usage: Usage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageCommit {
    pub session_id: SessionId,
    /// Interaction Turn this message was committed under. `None` for child
    /// agent Executions spawned by multi-agent tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<TurnId>,
    pub execution_id: ExecutionId,
    pub agent_instance_id: crate::AgentInstanceId,
    pub message_id: MessageId,
    pub parent_message_id: Option<MessageId>,
    pub message: Message,
    pub committed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitAck {
    pub session_id: SessionId,
    pub execution_id: ExecutionId,
    pub agent_instance_id: crate::AgentInstanceId,
    pub message_id: Option<MessageId>,
    /// Host-owned durable sequence / revision for this commit.
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CommitError {
    Unavailable,
    IdentityMismatch,
    SequenceMismatch { expected: u64, actual: u64 },
    IdempotencyConflict,
    Failed(String),
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "persistence is unavailable"),
            Self::IdentityMismatch => write!(f, "persistence identity mismatch"),
            Self::SequenceMismatch { expected, actual } => {
                write!(
                    f,
                    "persistence sequence mismatch: expected {expected}, got {actual}"
                )
            }
            Self::IdempotencyConflict => write!(f, "persistence idempotency conflict"),
            Self::Failed(msg) => write!(f, "persistence failed: {msg}"),
        }
    }
}

impl std::error::Error for CommitError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_execution_serializes_without_task_fields() {
        let value = serde_json::to_value(StartExecutionRequest {
            request_id: "req-1".into(),
            session_id: "session-1".into(),
            source_turn_id: Some("turn-1".into()),
            execution_id: "exec-1".into(),
            agent_instance_id: "root".into(),
            agent_spec: crate::AgentSpec {
                id: "main".into(),
                name: "main".into(),
                role: "test".into(),
                description: None,
                system_prompt: "test".into(),
                model: None,
                thinking_level: None,
                tool_set_ids: Vec::new(),
                active_tool_names: None,
            },
            input_message_id: "msg-1".into(),
            input: MessageContent::String("hi".into()),
            context: ConversationContext::empty(),
            config: ExecutionConfig::default(),
        })
        .unwrap();
        assert!(value.get("taskId").is_none());
        assert!(value.get("workId").is_none());
        assert_eq!(value["executionId"], "exec-1");
        assert_eq!(value["sourceTurnId"], "turn-1");
    }

    #[test]
    fn outcome_status_mapping() {
        assert_eq!(
            ExecutionOutcome::Succeeded {
                usage: Usage::default()
            }
            .status(),
            ExecutionStatus::Succeeded
        );
        assert_eq!(
            ExecutionOutcome::failed("boom").status(),
            ExecutionStatus::Failed
        );
        assert_eq!(
            ExecutionOutcome::Cancelled { reason: None }.status(),
            ExecutionStatus::Cancelled
        );
    }
}
