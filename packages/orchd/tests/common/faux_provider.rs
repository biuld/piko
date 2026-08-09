// ---- FauxProvider — mock LlmGateway for tests ----
//
// Mirrors the TS FauxProvider pattern. Returns canned responses without
// requiring real API keys or network access.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio_stream::iter;
use tokio_util::sync::CancellationToken;

use piko_llmd::gateway::{GatewayError, LlmGateway, ModelEvent, ModelEventStream, ModelRequest};
use piko_protocol::messages::{Message, Model, ToolCall, Usage};
use piko_protocol::model::{ModelCapabilities, ModelRunSettings};

/// A canned response that the FauxProvider will emit.
#[derive(Clone, Default)]
pub struct CannedResponse {
    /// Text content for the assistant message.
    pub text: String,
    /// Tool calls emitted after `text` (each as one complete chunk).
    pub tool_calls: Vec<ToolCall>,
    /// Stop reason. Default: "stop".
    pub stop_reason: Option<String>,
    pub wait_for_cancel: bool,
}

impl CannedResponse {
    /// Quick constructor: text-only response.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tool_calls: Vec::new(),
            ..Default::default()
        }
    }

    /// Quick constructor: one or more complete tool calls, stop reason
    /// "tool_use".
    pub fn tool_calls(calls: Vec<ToolCall>) -> Self {
        Self {
            text: String::new(),
            tool_calls: calls,
            stop_reason: Some("tool_use".into()),
            ..Default::default()
        }
    }

    pub fn waiting_for_cancel() -> Self {
        Self {
            wait_for_cancel: true,
            ..Default::default()
        }
    }
}

/// A mock `LlmGateway` that returns pre-configured canned responses.
///
/// Responses are consumed from an internal queue — you push them before
/// the test and they're returned in order.
pub struct FauxProvider {
    responses: Arc<Mutex<Vec<CannedResponse>>>,
    call_count: Arc<Mutex<u32>>,
    requests: Arc<Mutex<Vec<ModelRequest>>>,
}

impl FauxProvider {
    /// Create a new FauxProvider with no queued responses.
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Queue a canned response. Will be consumed on the next streaming call.
    pub async fn push_response(&self, response: CannedResponse) {
        self.responses.lock().await.push(response);
    }

    /// Queue a simple text response.
    pub async fn push_text(&self, text: impl Into<String>) {
        self.responses.lock().await.push(CannedResponse::text(text));
    }

    /// Get the current call count.
    pub async fn call_count(&self) -> u32 {
        *self.call_count.lock().await
    }

    pub async fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().await.clone()
    }
}

impl Default for FauxProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmGateway for FauxProvider {
    async fn execute(
        &self,
        req: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelEventStream, GatewayError> {
        // Check cancellation
        if cancel.is_cancelled() {
            return Err(GatewayError::cancelled("faux"));
        }

        // Increment call count
        {
            let mut count = self.call_count.lock().await;
            *count += 1;
        }
        self.requests.lock().await.push(req);

        // Get the next queued response (or a fallback)
        let canned = {
            let mut responses = self.responses.lock().await;
            if responses.is_empty() {
                CannedResponse::text("No responses queued".to_string())
            } else {
                responses.remove(0)
            }
        };
        if canned.wait_for_cancel {
            cancel.cancelled().await;
            return Err(GatewayError::cancelled("faux"));
        }

        // Build the sequence of gateway events from the canned response
        let events: Vec<ModelEvent> = {
            let mut evs = Vec::new();

            // Content delta for text
            if !canned.text.is_empty() {
                evs.push(ModelEvent::text(canned.text.clone()));
            }
            for call in &canned.tool_calls {
                evs.push(ModelEvent::function_call(
                    call.id.clone(),
                    call.name.clone(),
                    serde_json::to_string(&call.arguments).unwrap_or_default(),
                ));
            }

            // Usage (empty for faux)
            evs.push(ModelEvent::Usage(Usage::empty()));
            evs.push(ModelEvent::output_metadata());

            // Done
            let stop = canned.stop_reason.clone().unwrap_or_else(|| "stop".into());
            evs.push(ModelEvent::completed(stop));

            evs
        };

        let stream = iter(events);
        Ok(Box::pin(stream))
    }

    async fn llm_call(
        &self,
        _model: Model,
        _system_prompt: Option<String>,
        _messages: Vec<Message>,
        _settings: ModelRunSettings,
    ) -> Result<String, String> {
        let canned = {
            let mut responses = self.responses.lock().await;
            if responses.is_empty() {
                CannedResponse::text("No responses queued".to_string())
            } else {
                responses.remove(0)
            }
        };
        Ok(canned.text)
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            supports_tools: false,
            supports_sandbox: false,
            supports_mcp: false,
            tools: Vec::new(),
        }
    }
}
