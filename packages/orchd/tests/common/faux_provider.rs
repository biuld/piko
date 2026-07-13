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
use piko_protocol::model::{ModelCapabilities, ModelRunSettings};

/// A canned response that the FauxProvider will emit.
#[derive(Clone, Default)]
pub struct CannedResponse {
    /// Text content for the assistant message.
    pub text: String,
    /// Stop reason. Default: "stop".
    pub stop_reason: Option<String>,
    pub wait_for_cancel: bool,
}

impl CannedResponse {
    /// Quick constructor: text-only response.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
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
    requests: Arc<Mutex<Vec<GatewayRequest>>>,
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

            // Content delta for text
            if !canned.text.is_empty() {
                evs.push(GatewayEvent::ContentDelta(canned.text.clone()));
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
        ModelCapabilities {
            supports_tools: false,
            supports_sandbox: false,
            supports_mcp: false,
            tools: Vec::new(),
        }
    }
}
