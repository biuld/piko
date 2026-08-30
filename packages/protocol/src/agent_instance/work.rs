//! Primitive Agent work facts and derived work projections.
//!
//! These values deliberately describe admission and causal correlation. They
//! do not turn Run or Execution into a second authoritative state machine.

use serde::{Deserialize, Serialize};

use super::{AgentInputDelivery, AgentInstanceId, AgentInstanceLifecycle};
use crate::MessageContent;

pub type AgentInputId = String;
pub type RunId = String;
pub type ModelStepId = String;

/// The typed source of an admitted input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentInputOrigin {
    User,
    Agent,
    System,
}

/// The primitive input admitted to one AgentInstance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInput {
    pub input_id: AgentInputId,
    pub request_id: crate::agent_runtime::RequestId,
    pub session_id: crate::SessionId,
    pub agent_instance_id: AgentInstanceId,
    pub origin: AgentInputOrigin,
    pub delivery: AgentInputDelivery,
    pub content: MessageContent,
    pub submitted_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_agent_instance_id: Option<AgentInstanceId>,
    /// Recipient of the eventual detached completion, when this input was
    /// submitted as background Agent-to-Agent work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detached_recipient_agent_instance_id: Option<AgentInstanceId>,
}

impl AgentInput {
    /// Construct a client-origin user input. `request_id` is caller
    /// idempotency; `input_id` is derived from it for tests and helpers.
    pub fn user(
        request_id: impl Into<String>,
        session_id: impl Into<String>,
        agent_instance_id: impl Into<String>,
        delivery: AgentInputDelivery,
        content: MessageContent,
    ) -> Self {
        let request_id = request_id.into();
        Self {
            input_id: format!("input_{request_id}"),
            request_id: request_id.clone(),
            session_id: session_id.into(),
            agent_instance_id: agent_instance_id.into(),
            origin: AgentInputOrigin::User,
            delivery,
            content,
            submitted_at: chrono::Utc::now().timestamp_millis(),
            caller_agent_instance_id: None,
            detached_recipient_agent_instance_id: None,
        }
    }

    /// Build the canonical primitive proposal from the existing runtime input
    /// DTO. Prompt resources remain runtime-owned and are intentionally not
    /// copied into the durable fact.
    pub fn from_request(request: &crate::SendAgentInputRequest, submitted_at: i64) -> Self {
        Self {
            input_id: request.request_id.clone(),
            request_id: request.request_id.clone(),
            session_id: request.session_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            origin: if request.caller_agent_instance_id.is_some() {
                AgentInputOrigin::Agent
            } else if request.source_turn_id.is_some() {
                AgentInputOrigin::User
            } else {
                AgentInputOrigin::System
            },
            delivery: request.delivery,
            content: request.content.clone(),
            submitted_at,
            caller_agent_instance_id: request.caller_agent_instance_id.clone(),
            detached_recipient_agent_instance_id: None,
        }
    }

    /// Live runtime request for this primitive. Prompt resources stay off
    /// the durable fact.
    pub fn to_request(&self) -> crate::SendAgentInputRequest {
        crate::SendAgentInputRequest {
            request_id: self.request_id.clone(),
            session_id: self.session_id.clone(),
            agent_instance_id: self.agent_instance_id.clone(),
            caller_agent_instance_id: self.caller_agent_instance_id.clone(),
            source_turn_id: None,
            message_id: self.input_id.clone(),
            content: self.content.clone(),
            delivery: self.delivery,
            prompt_resources: None,
            active_tool_names: None,
        }
    }

    /// Stable, bounded text used by queue and steer projections.
    pub fn preview(&self) -> String {
        let text = match &self.content {
            MessageContent::String(text) => text.clone(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .map(crate::ContentBlock::text_projection)
                .collect::<Vec<_>>()
                .join("\n"),
        };
        const PREVIEW_LIMIT: usize = 160;
        if text.chars().count() <= PREVIEW_LIMIT {
            return text;
        }
        let mut preview = text.chars().take(PREVIEW_LIMIT - 1).collect::<String>();
        preview.push('…');
        preview
    }
}

/// Normalized state exposed by the host's derived active-work view.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorkViewState {
    Starting,
    Running,
    RequiresAction,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

/// Compact pending action information used by foreground projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingActionSummary {
    pub action_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveWorkSnapshot {
    pub root_input_id: AgentInputId,
    pub state: AgentWorkViewState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_model_step_id: Option<ModelStepId>,
    pub started_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputSummary {
    pub input_id: AgentInputId,
    pub origin: AgentInputOrigin,
    pub preview: String,
    pub admission_revision: u64,
    pub submitted_at: i64,
    pub delivery: AgentInputDelivery,
    pub disposition: crate::execution::AgentInputDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkSnapshot {
    pub agent_instance_id: AgentInstanceId,
    pub lifecycle: AgentInstanceLifecycle,
    pub foreground: crate::AgentForeground,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_work: Option<ActiveWorkSnapshot>,
    pub pending_steers: Vec<AgentInputSummary>,
    pub queued_inputs: Vec<AgentInputSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<PendingActionSummary>,
}

/// Canonical input admission command payload shared by runtime and host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputAdmission {
    pub input: AgentInput,
    pub disposition: crate::execution::AgentInputDisposition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_input_id: Option<AgentInputId>,
    pub admitted_at: i64,
}

/// Canonical transition command payload shared by runtime and host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputDispositionChange {
    pub agent_instance_id: AgentInstanceId,
    pub input_id: AgentInputId,
    pub disposition: crate::execution::AgentInputDisposition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_input_id: Option<AgentInputId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_step_id: Option<ModelStepId>,
    pub changed_at: i64,
}
