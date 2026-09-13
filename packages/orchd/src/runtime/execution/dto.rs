//! Internal ExecutionActor request/receipt DTOs.
//!
//! Per ADR-027 there are no Execution-addressed public commands: these types
//! flow only between the AgentActor and its root-keyed ExecutionActor inside
//! orchd. They are crate-private wire data, not client or host contracts.

use serde::{Deserialize, Serialize};

pub(super) type RequestId = String;
pub(super) type SessionId = String;
pub(super) type MessageId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConversationContext {
    pub messages: Vec<piko_protocol::Message>,
    pub head_message_id: Option<MessageId>,
}

impl ConversationContext {
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
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
pub(crate) struct StartExecutionRequest {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionReceipt {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub root_input_id: String,
    pub agent_instance_id: piko_protocol::AgentInstanceId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SteerExecutionRequest {
    pub request_id: RequestId,
    pub input_id: String,
    pub session_id: SessionId,
    pub root_input_id: String,
    pub message_id: MessageId,
    pub content: piko_protocol::MessageContent,
    pub submitted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CancelExecutionRequest {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub root_input_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CancelReceipt {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub root_input_id: String,
    pub accepted: bool,
}
