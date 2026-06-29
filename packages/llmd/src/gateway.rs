use async_trait::async_trait;
use futures_core::Stream;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

use piko_protocol::messages::{Message, Model, Usage};
use piko_protocol::model::ModelCapabilities;
use piko_protocol::tools::ToolDef;

// ---- LLM Gateway types ----

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
    /// Resolved thinking level (after thinking_level_map lookup).
    /// None = thinking disabled.
    pub thinking: Option<String>,
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
        /// Complete tool call arguments (genai accumulates deltas internally)
        args: serde_json::Value,
    },
    Usage(Usage),
    Done(String),
    Error(String),
}

// ---- LlmGateway trait ----

/// Pure infrastructure-level LLM Gateway — a port in hexagonal architecture.
///
/// `orchd` depends on this trait; `llmd` provides the implementation.
#[async_trait]
pub trait LlmGateway: Send + Sync {
    /// Execute a chat completion and return a stream of gateway events.
    async fn chat_stream(
        &self,
        req: GatewayRequest,
        cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String>;

    /// Execute a raw stateless chat completion (non-streaming).
    async fn llm_call(
        &self,
        model: Model,
        system_prompt: Option<String>,
        messages: Vec<Message>,
        settings: piko_protocol::model::ModelRunSettings,
    ) -> Result<String, String>;

    /// Get capabilities (tool support) for the configured providers.
    fn capabilities(&self) -> ModelCapabilities;
}
