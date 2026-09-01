//! Internal Execution capabilities supplied by hostd to AgentRuntime.

use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::AgentInputDispositionChange;
use piko_protocol::agent_work::{CommitAck, CommitError, MessageCommit, ModelStepCommit};
use serde::{Deserialize, Serialize};

use crate::AgentApiError;

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
#[serde(rename_all = "camelCase")]
pub struct ConversationContext {
    pub messages: Vec<piko_protocol::Message>,
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

/// Actor-only request used between AgentActor and its root-keyed execution
/// worker. It is not a client command or durable product handle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartExecutionRequest {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub agent_instance_id: piko_protocol::AgentInstanceId,
    pub agent_spec: piko_protocol::AgentSpec,
    pub run_prompt: piko_protocol::SemanticRunPrompt,
    pub tool_catalog: piko_protocol::ResolvedToolCatalog,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_state: Option<piko_protocol::Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inter_agent_completions: Vec<piko_protocol::Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_mentions: Vec<piko_protocol::Message>,
    pub input_message_id: MessageId,
    pub input: piko_protocol::MessageContent,
    pub context: ConversationContext,
    pub config: ExecutionConfig,
    pub root_input_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReceipt {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub root_input_id: String,
    pub agent_instance_id: piko_protocol::AgentInstanceId,
    pub status: ExecutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteerExecutionRequest {
    pub request_id: RequestId,
    pub input_id: String,
    pub session_id: SessionId,
    pub root_input_id: String,
    pub message_id: MessageId,
    pub content: piko_protocol::MessageContent,
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
pub enum CancelReason {
    UserRequested,
    SessionShutdown,
    RuntimeShutdown,
    Superseded,
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
    pub accepted: bool,
}

/// Host-owned deterministic prompt assembler. AgentRuntime resolves the tool
/// catalog first and freezes the returned prompt with that exact catalog.
#[async_trait]
pub trait PromptAssemblyPort: Send + Sync {
    async fn assemble_prompt(
        &self,
        request: piko_protocol::PromptAssemblyRequest,
    ) -> Result<piko_protocol::SemanticRunPrompt, AgentApiError>;
}

/// Durable commit port owned by hostd and scoped to a Session/Execution.
#[async_trait]
pub trait ExecutionCommitPort: Send + Sync {
    async fn commit_message(&self, commit: MessageCommit) -> Result<CommitAck, CommitError>;

    /// Atomically persist a steer message and the disposition transition that
    /// applies its input to the reserved next model step. A steer must never
    /// become visible as delivered without its causal input fact.
    async fn commit_steer(
        &self,
        message: MessageCommit,
        change: AgentInputDispositionChange,
    ) -> Result<CommitAck, CommitError>;

    /// Atomically commit one assistant response and its ordered tool
    /// declarations. The runtime must not execute those tools before this
    /// acknowledgement succeeds.
    async fn commit_model_step(&self, commit: ModelStepCommit) -> Result<CommitAck, CommitError>;
}

/// Approval requests addressed by Session + root AgentInput identity.
#[async_trait]
pub trait ApprovalPort: Send + Sync {
    async fn request_approval(
        &self,
        session_id: &str,
        root_input_id: &str,
        request: crate::ToolApprovalRequest,
    ) -> Result<crate::ToolApprovalDecision, AgentApiError>;
}

/// Interactive prompts addressed by Session + root AgentInput identity.
#[async_trait]
pub trait InteractionPort: Send + Sync {
    async fn request_interaction(
        &self,
        session_id: &str,
        root_input_id: &str,
        request: serde_json::Value,
    ) -> Result<serde_json::Value, AgentApiError>;
}

/// Lossy realtime fan-out. Must never block the ExecutionActor.
pub trait RealtimeDeltaSink: Send + Sync {
    fn try_publish(&self, delta: piko_protocol::agent_runtime::RealtimeDeltaEnvelope);
}

/// Best-effort durable trajectory capture (F-36). The hostd implementation
/// must never block, fail, or alter the turn: it enqueues into a bounded
/// channel and returns immediately.
#[async_trait]
pub trait TrajectoryCapturePort: Send + Sync {
    async fn record(&self, record: piko_protocol::TrajectoryRecord);
}

/// Immutable session-scoped capabilities for one attached Session.
pub struct SessionExecutionPorts {
    pub commit: Arc<dyn ExecutionCommitPort>,
    pub prompt: Option<Arc<dyn PromptAssemblyPort>>,
    pub approval: Option<Arc<dyn ApprovalPort>>,
    pub interaction: Option<Arc<dyn InteractionPort>>,
    pub realtime: Option<Arc<dyn RealtimeDeltaSink>>,
    pub trajectory: Option<Arc<dyn TrajectoryCapturePort>>,
}

impl SessionExecutionPorts {
    pub fn new(commit: Arc<dyn ExecutionCommitPort>) -> Self {
        Self {
            commit,
            prompt: None,
            approval: None,
            interaction: None,
            realtime: None,
            trajectory: None,
        }
    }

    pub fn with_prompt(mut self, prompt: Arc<dyn PromptAssemblyPort>) -> Self {
        self.prompt = Some(prompt);
        self
    }

    pub fn with_approval(mut self, approval: Arc<dyn ApprovalPort>) -> Self {
        self.approval = Some(approval);
        self
    }

    pub fn with_interaction(mut self, interaction: Arc<dyn InteractionPort>) -> Self {
        self.interaction = Some(interaction);
        self
    }

    pub fn with_realtime(mut self, realtime: Arc<dyn RealtimeDeltaSink>) -> Self {
        self.realtime = Some(realtime);
        self
    }

    pub fn with_trajectory(mut self, trajectory: Arc<dyn TrajectoryCapturePort>) -> Self {
        self.trajectory = Some(trajectory);
        self
    }
}
