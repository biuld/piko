// ---- Model: executor — ModelStepExecutor trait + self-llm adapter ----

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::protocol::config::ProviderConfig;
use crate::protocol::event_stream::{EventStream, EventStreamSender, create_event_stream};
use crate::protocol::messages::{ContentBlock, MessageContent, Usage};
use crate::protocol::model::ModelCapabilities;
use crate::protocol::runtime_stream::RuntimeAssistantContentBlock;

use super::types::*;

// ---- Trait ----

/// The trait that the orchestrator uses to call LLMs.
///
/// Each call to `execute_step` returns an `EventStream` of `ModelStepEvent`
/// values, with a final `ModelStepResult` available via `.result()`.
pub trait ModelStepExecutor: Send + Sync {
    /// Execute a single model step. Returns a stream of events + deferred final result.
    fn execute_step(
        &self,
        input: ModelStepInput,
        cancel: Option<CancellationToken>,
    ) -> EventStream<ModelStepEvent, ModelStepResult>;

    /// Get model capabilities (tool support, sandbox, etc.).
    fn capabilities(&self) -> ModelCapabilities;

    /// Shutdown the executor gracefully.
    fn shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

// ---- self-llm based executor ----

/// Shared state (Arc enables Clone into spawned tasks).
struct ExecState {
    provider_configs: HashMap<String, ProviderConfig>,
    tool_defs: Vec<crate::protocol::tools::ToolDef>,
}

/// A `ModelStepExecutor` backed by `self-llm`.
///
/// Wraps state in `Arc` so `execute_step` can clone and spawn tasks freely.
/// Provider credentials are provided at construction time; no env vars are read.
pub struct SelfLlmExecutor {
    state: Arc<ExecState>,
}

impl SelfLlmExecutor {
    /// Create a new executor with no configured providers.
    pub fn new() -> Self {
        Self {
            state: Arc::new(ExecState {
                provider_configs: HashMap::new(),
                tool_defs: vec![],
            }),
        }
    }

    /// Create from provider configs (the recommended way).
    pub fn from_providers(providers: HashMap<String, ProviderConfig>) -> Self {
        Self {
            state: Arc::new(ExecState {
                provider_configs: providers,
                tool_defs: vec![],
            }),
        }
    }

    /// Set tool definitions for capability reporting.
    pub fn with_tool_defs(mut self, defs: Vec<crate::protocol::tools::ToolDef>) -> Self {
        if let Some(state) = Arc::get_mut(&mut self.state) {
            state.tool_defs = defs;
        }
        self
    }

    /// Build a self_llm::Client for the given provider name.
    fn build_client(&self, provider: &str) -> Result<self_llm::Client, String> {
        let config = self
            .state
            .provider_configs
            .get(&provider.to_lowercase())
            .or_else(|| self.state.provider_configs.get(provider))
            .ok_or_else(|| format!("Provider not configured: {provider}"))?;

        let provider_type = match config.kind.to_lowercase().as_str() {
            "openai" | "openrouter" | "azure" | "groq" | "deepseek" => {
                self_llm::config::ProviderType::OpenAi
            }
            "anthropic" | "claude" => self_llm::config::ProviderType::Anthropic,
            _other => {
                // Default to OpenAI-compatible for custom providers
                self_llm::config::ProviderType::OpenAi
            }
        };

        let base_url =
            config
                .base_url
                .clone()
                .unwrap_or_else(|| match config.kind.to_lowercase().as_str() {
                    "openai" => "https://api.openai.com/v1".into(),
                    "anthropic" | "claude" => "https://api.anthropic.com".into(),
                    _ => format!("https://api.{}.com/v1", config.kind),
                });

        let llm_config = self_llm::config::LlmProviderConfig::new(
            &config.kind,
            base_url,
            provider_type,
            &config.api_key,
        )
        .custom_headers(config.headers.clone().unwrap_or_default());

        Ok(llm_config.build_client())
    }
}

impl Default for SelfLlmExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelStepExecutor for SelfLlmExecutor {
    fn execute_step(
        &self,
        input: ModelStepInput,
        cancel: Option<CancellationToken>,
    ) -> EventStream<ModelStepEvent, ModelStepResult> {
        let (sender, stream) = create_event_stream::<ModelStepEvent, ModelStepResult>(64);

        // Build client from stored provider config (no env vars)
        let client = match self.build_client(&input.model.provider) {
            Ok(c) => c,
            Err(err_msg) => {
                let (err_sender, err_stream) =
                    create_event_stream::<ModelStepEvent, ModelStepResult>(1);
                tokio::spawn(async move {
                    let _ = err_sender
                        .send(ModelStepEvent::Error { message: err_msg })
                        .await;
                    err_sender
                        .finalize(ModelStepResult {
                            status: "error".into(),
                            appended_messages: vec![],
                            transcript_delta: vec![],
                            stop_reason: "error".into(),
                            usage: None,
                            engine_state: None,
                        })
                        .ok();
                });
                return err_stream;
            }
        };

        // Clone all input fields for the spawned task (no borrows of `self`)
        let model_id = input.model.id.clone();
        let system_prompt = input.system_prompt.clone();
        let transcript = input.transcript.clone();
        let tools = input.tools.clone();
        let step_id = input.step_id.clone();
        let run_id = input.run_id.clone();

        let sender = Arc::new(tokio::sync::Mutex::new(sender));

        tokio::spawn(async move {
            let result = execute_llm_call(
                &client,
                &model_id,
                &system_prompt,
                &transcript,
                &tools,
                &step_id,
                &run_id,
                &sender,
                cancel.as_ref(),
            )
            .await;

            // Finalize — if Arc is still uniquely owned
            if let Ok(sender_mutex) = Arc::try_unwrap(sender) {
                sender_mutex.into_inner().finalize(result).ok();
            }
        });

        stream
    }

    fn capabilities(&self) -> ModelCapabilities {
        let supports_tools = !self.state.tool_defs.is_empty();
        ModelCapabilities {
            supports_tools,
            supports_sandbox: false,
            supports_mcp: false,
            tools: self
                .state
                .tool_defs
                .iter()
                .map(|t| crate::protocol::model::ToolInfo {
                    name: t.name.clone(),
                    description: t.description.clone(),
                })
                .collect(),
        }
    }

    fn shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}

// ---- LLM call implementation ----

async fn execute_llm_call(
    client: &self_llm::Client,
    model_id: &str,
    system_prompt: &str,
    transcript: &[crate::protocol::messages::Message],
    tools: &[crate::protocol::tools::ToolDef],
    step_id: &str,
    run_id: &str,
    sender: &Arc<tokio::sync::Mutex<EventStreamSender<ModelStepEvent, ModelStepResult>>>,
    cancel: Option<&CancellationToken>,
) -> ModelStepResult {
    let msg_id = super::types::runtime_assistant_message_id(run_id, step_id);

    // Check cancellation before any work
    if cancel.is_some_and(|c| c.is_cancelled()) {
        return ModelStepResult {
            status: "aborted".into(),
            appended_messages: vec![],
            transcript_delta: vec![],
            stop_reason: "abort".into(),
            usage: None,
            engine_state: None,
        };
    }

    // Build self-llm messages from transcript (structured tool calls + results)
    let llm_messages = build_llm_messages(system_prompt, transcript);

    // Emit step_start
    {
        let s = sender.lock().await;
        let _ = s.send(ModelStepEvent::StepStart).await;
    }

    // Build the chat request with tool definitions
    let mut request = self_llm::ChatRequest::new(model_id, llm_messages);
    if !tools.is_empty() {
        let llm_tools: Vec<self_llm::Tool> = tools.iter().map(orch_tool_to_self_llm).collect();
        request = request.tools(llm_tools);
    }

    // Make the call (non-streaming for MVP)
    let chat_result = client.chat(request).await;

    match chat_result {
        Ok(response) => {
            let stop_reason_str = format_stop_reason(&response.stop_reason);
            let timestamp = chrono::Utc::now().timestamp_millis();

            // Extract text and tool calls from response
            let text = response.text().unwrap_or_default().to_string();
            let tool_uses = response.tool_uses();

            // Build protocol content blocks (text blocks + tool call blocks)
            let mut protocol_blocks: Vec<ContentBlock> = Vec::new();
            let mut runtime_blocks: Vec<RuntimeAssistantContentBlock> = Vec::new();

            if !text.is_empty() {
                protocol_blocks.push(ContentBlock::Text { text: text.clone() });
                runtime_blocks.push(RuntimeAssistantContentBlock::Text { text: text.clone() });
            }

            for tu in &tool_uses {
                // Emit tool call delta events (so TUI can show them)
                {
                    let s = sender.lock().await;
                    let _ = s
                        .send(ModelStepEvent::ProviderToolCallDelta {
                            id: tu.id.clone(),
                            name: tu.name.clone(),
                            args_delta: None,
                        })
                        .await;
                }

                protocol_blocks.push(ContentBlock::ToolCall {
                    id: tu.id.clone(),
                    name: tu.name.clone(),
                    arguments: tu.input.clone(),
                    partial_json: None,
                });
                runtime_blocks.push(RuntimeAssistantContentBlock::ToolCall {
                    id: tu.id.clone(),
                    name: tu.name.clone(),
                    arguments: tu.input.clone(),
                    partial_json: None,
                });
            }

            // If model returned nothing (edge case), emit at least an empty text block
            if protocol_blocks.is_empty() {
                protocol_blocks.push(ContentBlock::Text {
                    text: String::new(),
                });
                runtime_blocks.push(RuntimeAssistantContentBlock::Text {
                    text: String::new(),
                });
            }

            // Build runtime assistant message
            let runtime_msg = crate::protocol::runtime_stream::RuntimeMessage::Assistant {
                id: msg_id.clone(),
                content: runtime_blocks,
                is_streaming: Some(false),
                stop_reason: stop_reason_str.clone(),
                error_message: None,
                usage: None,
                provider: Some("self-llm".into()),
                model: Some(model_id.to_string()),
                timestamp: Some(timestamp),
            };

            // Build protocol message for transcript append
            let protocol_msg = crate::protocol::messages::Message::Assistant {
                content: protocol_blocks.clone(),
                api: "openai-completions".into(),
                provider: "self-llm".into(),
                model: model_id.to_string(),
                usage: Some(Usage::empty()),
                stop_reason: Some("stop".into()),
                error_message: None,
                timestamp: Some(timestamp),
            };

            // Emit events
            {
                let s = sender.lock().await;
                let _ = s
                    .send(ModelStepEvent::MessageStart {
                        message: runtime_msg.clone(),
                    })
                    .await;
                let _ = s
                    .send(ModelStepEvent::MessageEnd {
                        message: runtime_msg,
                    })
                    .await;
                let _ = s.send(ModelStepEvent::StepEnd).await;
            }

            let transcript_msg = crate::protocol::messages::Message::Assistant {
                content: protocol_blocks,
                api: "openai-completions".into(),
                provider: "self-llm".into(),
                model: model_id.to_string(),
                usage: Some(Usage::empty()),
                stop_reason: Some("stop".into()),
                error_message: None,
                timestamp: None,
            };

            ModelStepResult {
                status: "completed".into(),
                appended_messages: vec![protocol_msg],
                transcript_delta: vec![TranscriptDelta::AssistantMessage {
                    message: transcript_msg,
                }],
                stop_reason: "assistant".into(),
                usage: Some(Usage::empty()),
                engine_state: None,
            }
        }
        Err(e) => {
            let error_msg = e.to_string();

            {
                let s = sender.lock().await;
                let _ = s
                    .send(ModelStepEvent::Error {
                        message: error_msg.clone(),
                    })
                    .await;
                let _ = s.send(ModelStepEvent::StepEnd).await;
            }

            ModelStepResult {
                status: "error".into(),
                appended_messages: vec![],
                transcript_delta: vec![],
                stop_reason: "error".into(),
                usage: None,
                engine_state: None,
            }
        }
    }
}

/// Format stop_reason from self-llm's response type.
fn format_stop_reason(sr: &self_llm::StopReason) -> Option<String> {
    Some(format!("{sr:?}"))
}

// ---- Message conversion helpers ----

/// Convert an orchd ToolDef to a self-llm Tool.
fn orch_tool_to_self_llm(tool: &crate::protocol::tools::ToolDef) -> self_llm::Tool {
    self_llm::Tool {
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters: tool.input_schema.clone(),
    }
}

/// Build self-llm messages from protocol messages.
///
/// Assistant messages include tool call blocks as structured `ContentPart::ToolUse`.
/// Tool results use `Message::tool_results()` for proper structured role=tool format.
fn build_llm_messages(
    system_prompt: &str,
    transcript: &[crate::protocol::messages::Message],
) -> Vec<self_llm::Message> {
    let mut messages: Vec<self_llm::Message> = Vec::with_capacity(transcript.len() + 1);

    if !system_prompt.is_empty() {
        messages.push(self_llm::Message::system(system_prompt));
    }

    for msg in transcript {
        let llm_msg = match msg {
            crate::protocol::messages::Message::User { content, .. } => {
                let text = extract_text(content);
                self_llm::Message::user(text)
            }
            crate::protocol::messages::Message::Assistant { content, .. } => {
                build_assistant_message(content)
            }
            crate::protocol::messages::Message::ToolResult {
                tool_call_id,
                content,
                is_error,
                ..
            } => {
                let text = extract_blocks(content);
                self_llm::Message::tool_results(vec![self_llm::ToolResult {
                    tool_use_id: tool_call_id.clone(),
                    content: text,
                    is_error: *is_error == Some(true),
                }])
            }
        };
        messages.push(llm_msg);
    }

    messages
}

/// Build a self-llm assistant message with proper structured content blocks.
fn build_assistant_message(content: &[ContentBlock]) -> self_llm::Message {
    let mut parts: Vec<self_llm::ContentPart> = Vec::with_capacity(content.len());

    for block in content {
        match block {
            ContentBlock::Text { text } => {
                parts.push(self_llm::ContentPart::Text(text.clone()));
            }
            ContentBlock::Thinking { thinking, .. } => {
                parts.push(self_llm::ContentPart::Reasoning(thinking.clone()));
            }
            ContentBlock::ToolCall {
                id,
                name,
                arguments,
                ..
            } => {
                parts.push(self_llm::ContentPart::ToolUse(self_llm::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: arguments.clone(),
                }));
            }
            // Images are not yet supported in transcript conversion
            ContentBlock::Image { .. } => {}
        }
    }

    self_llm::Message {
        role: self_llm::Role::Assistant,
        content: parts,
    }
}

fn extract_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(s) => s.clone(),
        MessageContent::Blocks(blocks) => extract_blocks(blocks),
    }
}

fn extract_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
