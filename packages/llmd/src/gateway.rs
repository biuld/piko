use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use piko_protocol::messages::{Message, Model, Usage};
use piko_protocol::model::ModelCapabilities;
use piko_protocol::tools::ToolDef;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub session_id: String,
    pub agent_instance_id: String,
    pub provider: String,
    pub model: String,
    pub run_prompt: piko_protocol::SemanticRunPrompt,
    pub transcript: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub run_id: String,
    pub step_id: String,
    pub thinking: Option<String>,
}

pub type ModelEventStream = Pin<Box<dyn Stream<Item = ModelEvent> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemIdentity {
    pub response_id: Option<String>,
    pub item_id: Option<String>,
    pub call_id: Option<String>,
    pub output_index: Option<u32>,
    pub content_index: Option<u32>,
}

impl ItemIdentity {
    pub fn none() -> Self {
        Self {
            response_id: None,
            item_id: None,
            call_id: None,
            output_index: None,
            content_index: None,
        }
    }

    pub fn call(call_id: impl Into<String>) -> Self {
        Self {
            response_id: None,
            item_id: None,
            call_id: Some(call_id.into()),
            output_index: None,
            content_index: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalStatus {
    Completed { reason: String },
    Incomplete { reason: Option<String> },
    Failed { message: String },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticItem {
    Text {
        text: String,
        identity: ItemIdentity,
    },
    Refusal {
        text: String,
        identity: ItemIdentity,
    },
    Reasoning {
        text: String,
        identity: ItemIdentity,
    },
    FunctionCall {
        name: String,
        arguments: String,
        identity: ItemIdentity,
    },
    FunctionResult {
        output: String,
        identity: ItemIdentity,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOutputMetadata {
    /// Adapter-produced continuation. Runtime consumers carry this value
    /// without interpreting or constructing protocol-specific state.
    pub continuation: Option<piko_protocol::ModelContinuation>,
}

impl ModelOutputMetadata {
    pub fn without_continuation() -> Self {
        Self { continuation: None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelResult {
    pub items: Vec<SemanticItem>,
    pub usage: Option<Usage>,
    pub status: TerminalStatus,
    pub output_metadata: ModelOutputMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelEvent {
    OutputMetadata(ModelOutputMetadata),
    TextDelta {
        delta: String,
        identity: ItemIdentity,
    },
    RefusalDelta {
        delta: String,
        identity: ItemIdentity,
    },
    ReasoningDelta {
        delta: String,
        identity: ItemIdentity,
    },
    FunctionCallDelta {
        name: String,
        arguments_delta: String,
        identity: ItemIdentity,
    },
    Usage(Usage),
    Completed(TerminalStatus),
    Error(GatewayError),
}

impl ModelEvent {
    pub fn text(delta: impl Into<String>) -> Self {
        Self::TextDelta {
            delta: delta.into(),
            identity: ItemIdentity::none(),
        }
    }

    pub fn reasoning(delta: impl Into<String>) -> Self {
        Self::ReasoningDelta {
            delta: delta.into(),
            identity: ItemIdentity::none(),
        }
    }

    pub fn function_call(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments_delta: impl Into<String>,
    ) -> Self {
        Self::FunctionCallDelta {
            name: name.into(),
            arguments_delta: arguments_delta.into(),
            identity: ItemIdentity::call(id),
        }
    }

    pub fn output_metadata() -> Self {
        Self::OutputMetadata(ModelOutputMetadata::without_continuation())
    }

    pub fn completed(reason: impl Into<String>) -> Self {
        Self::Completed(TerminalStatus::Completed {
            reason: reason.into(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Target,
    UnsupportedCapability,
    Authentication,
    Transport,
    Timeout,
    RateLimit,
    Upstream,
    Sse,
    Protocol,
    Cancelled,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{target} {operation}: {message}")]
pub struct GatewayError {
    pub class: ErrorClass,
    pub target: String,
    pub operation: &'static str,
    pub message: String,
    pub status: Option<u16>,
    pub request_id: Option<String>,
    pub retry_after_ms: Option<u64>,
}

impl GatewayError {
    pub fn new(
        class: ErrorClass,
        target: impl Into<String>,
        operation: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            class,
            target: target.into(),
            operation,
            message: message.into(),
            status: None,
            request_id: None,
            retry_after_ms: None,
        }
    }

    pub fn cancelled(target: impl Into<String>) -> Self {
        Self::new(ErrorClass::Cancelled, target, "execute", "cancelled")
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self.class,
            ErrorClass::Transport | ErrorClass::Timeout | ErrorClass::RateLimit
        ) || self
            .status
            .is_some_and(|status| matches!(status, 408 | 409 | 425 | 429 | 500 | 502 | 503 | 504))
    }
}

#[async_trait]
pub trait LlmGateway: Send + Sync {
    async fn execute(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelEventStream, GatewayError>;

    async fn execute_once(
        &self,
        _request: ModelRequest,
        _cancel: CancellationToken,
    ) -> Result<ModelResult, GatewayError> {
        Err(GatewayError::new(
            ErrorClass::UnsupportedCapability,
            "gateway",
            "execute_once",
            "non-streaming execution is not implemented",
        ))
    }

    async fn llm_call(
        &self,
        _model: Model,
        _system_prompt: Option<String>,
        _messages: Vec<Message>,
        _settings: piko_protocol::model::ModelRunSettings,
    ) -> Result<String, String> {
        Err("stateless call is not implemented".into())
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
}
