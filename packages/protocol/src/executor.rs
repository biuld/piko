use std::future::Future;
use std::pin::Pin;
use futures_core::Stream;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::messages::{Message, Usage};
use crate::model::ModelCapabilities;
use crate::tools::ToolDef;

// ---- Gateway Port (Interface) ----

/// A simple, stateless request to the LLM Gateway.
#[derive(Debug, Clone)]
pub struct GatewayRequest {
    pub provider: String,
    pub model: String,
    pub system_prompt: String,
    pub transcript: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub run_id: String,
    pub step_id: String,
}

/// A streaming event from the LLM Gateway.
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    ContentDelta(String),
    ReasoningDelta(String),
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolCallDelta {
        index: usize,
        arguments_delta: String,
    },
    Usage(Usage),
    Done(String), // stop_reason
    Error(String),
}

/// Pure infrastructure-level LLM Gateway Trait.
#[async_trait]
pub trait LlmGateway: Send + Sync {
    /// Execute a chat completion and return a stream of gateway events.
    async fn chat_stream(
        &self,
        req: GatewayRequest,
        cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String>;

    /// Execute a raw stateless chat completion (non-streaming, used for compaction etc).
    fn llm_call(
        &self,
        model: crate::messages::Model,
        system_prompt: Option<String>,
        messages: Vec<crate::messages::Message>,
        settings: crate::model::ModelRunSettings,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>>;

    /// Get capabilities (tool support) for the configured providers.
    fn capabilities(&self) -> ModelCapabilities;
}
