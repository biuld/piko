pub mod cost_tracker;
pub mod token_usage;

use std::collections::HashMap;
use async_trait::async_trait;

/// Request-level context passed through the middleware chain
#[derive(Debug, Clone, Default)]
pub struct GatewayContext {
    pub run_id: String,
    pub step_id: String,
    pub model_id: String,
    pub provider: String,
    /// Mutable metadata store for middlewares to share data (e.g., costs, trace IDs)
    pub metadata: HashMap<String, String>,
}

use piko_protocol::executor::GatewayEvent;

/// A filter chain / interceptor hook for LLM requests
#[async_trait]
pub trait LlmdMiddleware: Send + Sync {
    /// Called before the request is sent to the LLM provider.
    /// Return `Err(String)` to short-circuit and abort the request.
    async fn pre_chat(
        &self,
        _ctx: &mut GatewayContext,
        _request: &mut self_llm::ChatRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Called when an event is received from the LLM provider stream.
    async fn on_stream_event(
        &self,
        _ctx: &mut GatewayContext,
        _event: &mut GatewayEvent,
    ) -> Result<(), String> {
        Ok(())
    }
}
