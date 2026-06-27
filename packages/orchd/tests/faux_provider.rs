// ---- Phase 8: FauxProvider — mock ModelStepExecutor for tests ----
//
// Mirrors the TS FauxProvider pattern. Returns canned responses without
// requiring real API keys or network access.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use orchd::model::executor::ModelStepExecutor;
use orchd::model::types::{
    ModelRuntimeCounters, ModelStepEvent, ModelStepInput, ModelStepResult,
    runtime_assistant_message_id,
};
use orchd::protocol::messages::{ContentBlock, Message, Usage};
use orchd::protocol::model::{ModelCapabilities, ToolInfo};
use orchd::protocol::tools::ToolDef;
use orchd::stream::{EventStream, create_event_stream};
use orchd::stream::{RuntimeAssistantContentBlock, RuntimeMessage};

/// A canned response that the FauxProvider will emit.
#[derive(Clone, Default)]
pub struct CannedResponse {
    /// Text content for the assistant message.
    pub text: String,
    /// Optional tool calls to emit.
    pub tool_calls: Vec<CannedToolCall>,
    /// Status for the step result. Default: "completed".
    pub status: Option<String>,
    /// Stop reason. Default: "assistant".
    pub stop_reason: Option<String>,
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
        }
    }
}

/// A mock `ModelStepExecutor` that returns pre-configured canned responses.
///
/// Responses are consumed from an internal queue — you push them before
/// the test and they're returned in order.
pub struct FauxProvider {
    responses: Arc<Mutex<Vec<CannedResponse>>>,
    call_count: Arc<Mutex<u32>>,
    /// Simple tool defs for capability reporting
    tool_defs: Arc<Vec<ToolDef>>,
}

impl FauxProvider {
    /// Create a new FauxProvider with no queued responses.
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
            tool_defs: Arc::new(Vec::new()),
        }
    }

    /// Create a FauxProvider with tool definitions for capability reporting.
    #[allow(dead_code)]
    pub fn with_tools(tools: Vec<ToolDef>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
            tool_defs: Arc::new(tools),
        }
    }

    /// Queue a canned response. Will be consumed on the next `execute_step` call.
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
}

impl Default for FauxProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelStepExecutor for FauxProvider {
    fn execute_step(
        &self,
        input: ModelStepInput,
        cancel: Option<CancellationToken>,
    ) -> EventStream<ModelStepEvent, ModelStepResult> {
        let (sender, stream) = create_event_stream::<ModelStepEvent, ModelStepResult>(64);
        let response_cell = self.responses.clone();
        let call_count = self.call_count.clone();

        tokio::spawn(async move {
            // Increment call count
            {
                let mut count = call_count.lock().await;
                *count += 1;
            }

            // Check cancellation
            if cancel.is_some_and(|c| c.is_cancelled()) {
                sender
                    .send(ModelStepEvent::Error {
                        message: "Task cancelled".into(),
                    })
                    .await
                    .ok();
                sender
                    .finalize(ModelStepResult {
                        status: "aborted".into(),
                        appended_messages: vec![],
                        transcript_delta: vec![],
                        stop_reason: "abort".into(),
                        error_message: None,
                        usage: None,
                        engine_state: None,
                    })
                    .ok();
                return;
            }

            // Get the next queued response (or a fallback)
            let canned = {
                let mut responses = response_cell.lock().await;
                if responses.is_empty() {
                    // Fallback: return a simple text response
                    CannedResponse::text("No responses queued".to_string())
                } else {
                    responses.remove(0)
                }
            };

            let msg_id = runtime_assistant_message_id(&input.run_id, &input.step_id);
            let step_counters = build_counters(&input);

            // Emit step_start
            sender.send(ModelStepEvent::StepStart).await.ok();

            // Build assistant message content blocks
            let mut content_blocks: Vec<RuntimeAssistantContentBlock> = Vec::new();

            if !canned.text.is_empty() {
                content_blocks.push(RuntimeAssistantContentBlock::Text {
                    text: canned.text.clone(),
                });
            }

            for tc in &canned.tool_calls {
                content_blocks.push(RuntimeAssistantContentBlock::ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    partial_json: None,
                });
            }

            let runtime_msg = RuntimeMessage::Assistant {
                id: msg_id.clone(),
                content: content_blocks,
                is_streaming: Some(false),
                stop_reason: canned.stop_reason.clone(),
                error_message: None,
                usage: Some(Usage::empty()),
                provider: Some("faux".into()),
                model: Some(input.model.id.clone()),
                timestamp: Some(chrono::Utc::now().timestamp_millis()),
            };

            // Emit message_start
            sender
                .send(ModelStepEvent::MessageStart {
                    message: runtime_msg.clone(),
                })
                .await
                .ok();

            // Emit message_end
            sender
                .send(ModelStepEvent::MessageEnd {
                    message: runtime_msg.clone(),
                })
                .await
                .ok();

            // Emit step_end
            sender.send(ModelStepEvent::StepEnd).await.ok();

            // Build protocol message for transcript
            let mut message_blocks: Vec<ContentBlock> = Vec::new();
            if !canned.text.is_empty() {
                message_blocks.push(ContentBlock::Text {
                    text: canned.text.clone(),
                });
            }
            for tc in &canned.tool_calls {
                message_blocks.push(ContentBlock::ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    partial_json: None,
                });
            }

            let transcript_msg = Message::Assistant {
                content: message_blocks,
                api: "faux".into(),
                provider: "faux".into(),
                model: input.model.id.clone(),
                usage: Some(Usage::empty()),
                stop_reason: canned.stop_reason.clone().or(Some("stop".into())),
                error_message: None,
                timestamp: Some(chrono::Utc::now().timestamp_millis()),
            };

            let model_stop_reason = canned
                .stop_reason
                .clone()
                .unwrap_or_else(|| "assistant".into());
            sender
                .finalize(ModelStepResult {
                    status: canned.status.unwrap_or_else(|| "completed".into()),
                    appended_messages: vec![transcript_msg],
                    transcript_delta: vec![],
                    stop_reason: model_stop_reason,
                    error_message: None,
                    usage: Some(Usage::empty()),
                    engine_state: Some(orchd::model::types::ModelContinuationState::ready(
                        step_counters,
                    )),
                })
                .ok();
        });

        stream
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

    fn shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }

    fn llm_call(
        &self,
        _model: orchd::protocol::messages::Model,
        _system_prompt: Option<String>,
        _messages: Vec<orchd::protocol::messages::Message>,
        _settings: orchd::protocol::model::ModelRunSettings,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>> {
        let response_cell = self.responses.clone();
        let call_count = self.call_count.clone();

        Box::pin(async move {
            {
                let mut count = call_count.lock().await;
                *count += 1;
            }
            let canned = {
                let mut responses = response_cell.lock().await;
                if responses.is_empty() {
                    CannedResponse::text("No responses queued".to_string())
                } else {
                    responses.remove(0)
                }
            };
            Ok(canned.text)
        })
    }
}

fn build_counters(input: &ModelStepInput) -> ModelRuntimeCounters {
    let prev = orchd::model::types::ModelContinuationState::extract(input.engine_state.as_ref());
    match prev {
        Some(state) => {
            let mut counters = state.counters;
            counters.model_calls += 1;
            counters
        }
        None => {
            let mut counters = ModelRuntimeCounters::new();
            counters.model_calls = 1;
            counters
        }
    }
}
