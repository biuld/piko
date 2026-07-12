// ---- FauxProvider — mock LlmGateway for tests ----
//
// Mirrors the TS FauxProvider pattern. Returns canned responses without
// requiring real API keys or network access.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::Mutex;
use tokio_stream::iter;
use tokio_util::sync::CancellationToken;

use llmd::gateway::{GatewayEvent, GatewayRequest, LlmGateway};
use piko_protocol::messages::{Message, Model, Usage};
use piko_protocol::model::{ModelCapabilities, ModelRunSettings, ToolInfo};
use piko_protocol::tools::ToolDef;

/// A canned response that the FauxProvider will emit.
#[derive(Clone, Default)]
pub struct CannedResponse {
    /// Text content for the assistant message.
    pub text: String,
    /// Optional tool calls to emit.
    pub tool_calls: Vec<CannedToolCall>,
    /// Status for the step result. Default: "completed".
    #[allow(dead_code)]
    pub status: Option<String>,
    /// Stop reason. Default: "stop".
    pub stop_reason: Option<String>,
    pub wait_for_cancel: bool,
}

#[derive(Clone)]
pub struct CannedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

impl CannedResponse {
    /// Quick constructor: text-only response.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }

    /// Constructor with tool calls.
    #[allow(dead_code)]
    pub fn with_tools(text: impl Into<String>, tools: Vec<CannedToolCall>) -> Self {
        Self {
            text: text.into(),
            tool_calls: tools,
            ..Default::default()
        }
    }

    /// Error response (simulates model failure).
    #[allow(dead_code)]
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            tool_calls: vec![],
            status: Some("error".into()),
            stop_reason: Some(format!("error: {}", msg.into())),
            wait_for_cancel: false,
        }
    }

    #[allow(dead_code)]
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
    requests: Arc<Mutex<Vec<GatewayRequest>>>,
    tool_defs: Arc<Vec<ToolDef>>,
}

impl FauxProvider {
    /// Create a new FauxProvider with no queued responses.
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
            requests: Arc::new(Mutex::new(Vec::new())),
            tool_defs: Arc::new(Vec::new()),
        }
    }

    /// Create a FauxProvider with tool definitions for capability reporting.
    #[allow(dead_code)]
    pub fn with_tools(tools: Vec<ToolDef>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
            requests: Arc::new(Mutex::new(Vec::new())),
            tool_defs: Arc::new(tools),
        }
    }

    /// Queue a canned response. Will be consumed on the next streaming call.
    #[allow(dead_code)]
    pub async fn push_response(&self, response: CannedResponse) {
        self.responses.lock().await.push(response);
    }

    /// Queue a simple text response.
    pub async fn push_text(&self, text: impl Into<String>) {
        self.responses.lock().await.push(CannedResponse::text(text));
    }

    /// Queue an error response (simulates model API error).
    #[allow(dead_code)]
    pub async fn push_error(&self, error_msg: impl Into<String>) {
        self.responses
            .lock()
            .await
            .push(CannedResponse::error(error_msg));
    }

    /// Get the current call count.
    #[allow(dead_code)]
    pub async fn call_count(&self) -> u32 {
        *self.call_count.lock().await
    }

    #[allow(dead_code)]
    pub async fn requests(&self) -> Vec<GatewayRequest> {
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
    async fn chat_stream(
        &self,
        req: GatewayRequest,
        cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        // Check cancellation
        if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            return Err("cancelled".into());
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
            if let Some(cancel) = cancel {
                cancel.cancelled().await;
                return Err("cancelled".into());
            }
            std::future::pending::<()>().await;
        }

        // Build the sequence of gateway events from the canned response
        let events: Vec<GatewayEvent> = {
            let mut evs = Vec::new();

            if canned.status.as_deref() == Some("error") {
                let error = canned
                    .stop_reason
                    .as_deref()
                    .and_then(|reason| reason.strip_prefix("error: "))
                    .unwrap_or("model error")
                    .to_string();
                evs.push(GatewayEvent::Error(error));
                return Ok(Box::pin(iter(evs)));
            }

            // Content delta for text
            if !canned.text.is_empty() {
                evs.push(GatewayEvent::ContentDelta(canned.text.clone()));
            }

            // Tool call events
            for tc in &canned.tool_calls {
                evs.push(GatewayEvent::ToolCallChunk {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    args_delta: serde_json::to_string(&tc.arguments).unwrap_or_default(),
                });
            }

            // Usage (empty for faux)
            evs.push(GatewayEvent::Usage(Usage::empty()));

            // Done
            let stop = canned.stop_reason.clone().unwrap_or_else(|| "stop".into());
            evs.push(GatewayEvent::Done(stop));

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
        let tools: Vec<ToolInfo> = self
            .tool_defs
            .iter()
            .map(|t| ToolInfo {
                name: t.name.clone(),
                description: t.description.clone(),
            })
            .collect();

        ModelCapabilities {
            supports_tools: !tools.is_empty(),
            supports_sandbox: false,
            supports_mcp: false,
            tools,
        }
    }
}
