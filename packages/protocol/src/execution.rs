//! Single-agent Execution DTOs.
//!
//! These types describe the operational Execution projection of a causal
//! AgentInput/Run and its ModelStep boundaries. The primitive input and work
//! contracts live under [`crate::agent_instance`].

use serde::{Deserialize, Serialize};

use crate::{Message, MessageContent, Usage};

pub type RequestId = String;
pub type SessionId = String;
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
}

impl ConversationContext {
    pub fn empty() -> Self {
        Self {
            messages: Vec::new(),
            head_message_id: None,
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
    pub agent_instance_id: crate::AgentInstanceId,
    pub agent_spec: crate::AgentSpec,
    pub run_prompt: crate::SemanticRunPrompt,
    pub tool_catalog: crate::ResolvedToolCatalog,
    /// Retained world-state Context message injected before the run input
    /// (F-04 slice 2). `None` for child agent runs without host resources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_state: Option<Message>,
    /// Unread detached inter-agent completions (F-20). Committed after
    /// `world_state` and before user mentions / the run input, in vector order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inter_agent_completions: Vec<Message>,
    /// File/skill mention Context messages (F-03 / D-27). Committed after
    /// inter-agent completions and before the run input.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_mentions: Vec<Message>,
    pub input_message_id: MessageId,
    pub input: MessageContent,
    pub context: ConversationContext,
    pub config: ExecutionConfig,
    /// Canonical AgentInput identity for the root of this work.
    pub root_input_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReceipt {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub root_input_id: String,
    pub agent_instance_id: crate::AgentInstanceId,
    pub status: ExecutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteerExecutionRequest {
    pub request_id: RequestId,
    /// Durable AgentInput identity. Compatibility callers may omit it while
    /// request_id remains the input identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_id: Option<String>,
    pub session_id: SessionId,
    pub root_input_id: String,
    pub message_id: MessageId,
    pub content: MessageContent,
    pub submitted_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InputDisposition {
    Accepted,
    Queued,
    Duplicate,
    Overload,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionInputReceipt {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub root_input_id: String,
    pub message_id: MessageId,
    pub disposition: InputDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CancelExecutionRequest {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub root_input_id: String,
    pub reason: CancelReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CancelReceipt {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub root_input_id: String,
    /// Actor accepted cancellation intent; terminal outcome is separate.
    pub accepted: bool,
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

/// Result of the model side of one Execution step. Tool execution starts only
/// after a `ToolCalls` step has been durably acknowledged.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModelStepOutcome {
    Completed,
    ToolCalls,
    Failed,
    Cancelled,
}

/// Atomic durable commit for one model request/response boundary.
///
/// The nested message commits carry the message bodies. The host persists
/// them together with one required model-step relation so replay never has to
/// infer a step from adjacent transcript messages.
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
/// fetched through the ordinary transcript projection by these IDs.
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
    fn start_execution_serializes_without_task_fields() {
        let value = serde_json::to_value(StartExecutionRequest {
            request_id: "req-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            agent_spec: crate::AgentSpec {
                id: "main".into(),
                version: "1".into(),
                provenance: crate::PromptSource::new("test", "main"),
                name: "main".into(),
                role: "test".into(),
                kind: crate::AgentKind::Supervisor,
                description: None,
                base_instructions: "test".into(),
                model: None,
                thinking_level: None,
                tool_set_ids: Vec::new(),
                active_tool_names: None,
            },
            run_prompt: crate::SemanticRunPrompt {
                assembly_version: crate::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
                source_digest: "digest".into(),
                ..Default::default()
            },
            tool_catalog: crate::ResolvedToolCatalog::default(),
            world_state: None,
            inter_agent_completions: Vec::new(),
            user_mentions: Vec::new(),
            input_message_id: "msg-1".into(),
            input: MessageContent::String("hi".into()),
            context: ConversationContext::empty(),
            config: ExecutionConfig::default(),
            root_input_id: "input-1".into(),
        })
        .unwrap();
        assert!(value.get("taskId").is_none());
        assert!(value.get("workId").is_none());
        assert_eq!(value["rootInputId"], "input-1");
        assert!(value.get("interAgentCompletions").is_none());
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
