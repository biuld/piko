//! Shared durable AgentInput work and ModelStep commit DTOs.
//!
//! Actor requests and receipts belong to `piko-orchd-api`; this module carries
//! only host/orchestrator DTOs that cross the persistence boundary.

use serde::{Deserialize, Serialize};

use crate::{Message, Usage};

pub type SessionId = String;
pub type MessageId = String;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentWorkProcessingStatus {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentWorkOutcome {
    Succeeded { usage: Usage },
    Failed { error: String },
    Cancelled { reason: Option<String> },
}

impl AgentWorkOutcome {
    pub fn failed(error: impl Into<String>) -> Self {
        Self::Failed {
            error: error.into(),
        }
    }

    pub fn status(&self) -> AgentWorkProcessingStatus {
        match self {
            Self::Succeeded { .. } => AgentWorkProcessingStatus::Succeeded,
            Self::Failed { .. } => AgentWorkProcessingStatus::Failed,
            Self::Cancelled { .. } => AgentWorkProcessingStatus::Cancelled,
        }
    }
}

/// Durable lifecycle state of an admitted AgentInput.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentInputDisposition {
    PendingFollowUp,
    PendingSteer,
    AppliedAsRoot,
    AppliedToStep,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageCommit {
    pub session_id: SessionId,
    pub root_input_id: String,
    pub agent_instance_id: crate::AgentInstanceId,
    pub message_id: MessageId,
    pub parent_message_id: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree_parent_entry_id: Option<MessageId>,
    pub message: Message,
    pub committed_at: i64,
}

/// Result of one model request. Tool execution starts only after a
/// `ToolCalls` step has been durably acknowledged.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModelStepOutcome {
    Completed,
    ToolCalls,
    Failed,
    Cancelled,
}

/// Atomic durable commit for one model request/response boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelStepCommit {
    pub session_id: SessionId,
    pub root_input_id: String,
    pub agent_instance_id: crate::AgentInstanceId,
    pub model_step_id: String,
    pub step_index: u32,
    pub started_at: i64,
    pub finished_at: i64,
    pub outcome: ModelStepOutcome,
    pub assistant: MessageCommit,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<MessageCommit>,
}

/// Durable/reliable identity of a committed model step. Message bodies are
/// fetched through the transcript projection by these IDs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelStepBoundary {
    pub session_id: SessionId,
    pub root_input_id: String,
    pub agent_instance_id: crate::AgentInstanceId,
    pub model_step_id: String,
    pub step_index: u32,
    pub started_at: i64,
    pub finished_at: i64,
    pub outcome: ModelStepOutcome,
    pub assistant_message_id: MessageId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_call_message_ids: Vec<MessageId>,
}

impl ModelStepCommit {
    pub fn boundary(&self) -> ModelStepBoundary {
        ModelStepBoundary {
            session_id: self.session_id.clone(),
            root_input_id: self.root_input_id.clone(),
            agent_instance_id: self.agent_instance_id.clone(),
            model_step_id: self.model_step_id.clone(),
            step_index: self.step_index,
            started_at: self.started_at,
            finished_at: self.finished_at,
            outcome: self.outcome,
            assistant_message_id: self.assistant.message_id.clone(),
            tool_call_message_ids: self
                .tool_calls
                .iter()
                .map(|commit| commit.message_id.clone())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitAck {
    pub session_id: SessionId,
    pub root_input_id: String,
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
    fn outcome_status_mapping() {
        assert_eq!(
            AgentWorkOutcome::Succeeded {
                usage: Usage::default()
            }
            .status(),
            AgentWorkProcessingStatus::Succeeded
        );
        assert_eq!(
            AgentWorkOutcome::failed("boom").status(),
            AgentWorkProcessingStatus::Failed
        );
        assert_eq!(
            AgentWorkOutcome::Cancelled { reason: None }.status(),
            AgentWorkProcessingStatus::Cancelled
        );
    }
}
