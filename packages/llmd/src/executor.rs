use std::collections::HashMap;
use std::sync::Arc;
use std::pin::Pin;

use async_trait::async_trait;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use piko_protocol::config::ProviderConfig;
use piko_protocol::messages::{ContentBlock, MessageContent, Usage};
use piko_protocol::model::ModelCapabilities;

use crate::gateway::{GatewayEvent, GatewayRequest, LlmGateway};

// ---- genai-based executor ----

/// Maps a provider kind to a genai AdapterKind.
fn adapter_kind(provider: &str) -> genai::adapter::AdapterKind {
    match provider.to_lowercase().as_str() {
        "openai" | "azure" | "groq" | "deepseek" | "openrouter" => {
            genai::adapter::AdapterKind::OpenAI
        }
        "anthropic" | "claude" => genai::adapter::AdapterKind::Anthropic,
        "gemini" | "google" => genai::adapter::AdapterKind::Gemini,
        _ => genai::adapter::AdapterKind::OpenAI,
    }
}

/// Build a genai Client with API keys and custom endpoints from our ProviderConfig map.
fn build_genai_client(providers: &HashMap<String, ProviderConfig>) -> genai::Client {
    // Clone the map for the closures
    let configs = providers.clone();

    // Auth resolver: returns API key for configured providers, falls back to env vars
    let configs_for_auth = configs.clone();
    let auth_resolver = genai::resolver::AuthResolver::from_resolver_fn(
        move |model_iden: genai::ModelIden| {
            let provider = provider_for_adapter(model_iden.adapter_kind);
            let result: std::result::Result<Option<genai::resolver::AuthData>, genai::resolver::Error> =
                if let Some(cfg) = configs_for_auth.get(&provider) {
                    if !cfg.api_key.is_empty() {
                        Ok(Some(genai::resolver::AuthData::Key(cfg.api_key.clone())))
                    } else {
                        // Fall through to env var
                        Ok(None)
                    }
                } else {
                    Ok(None)
                };
            result
        },
    );

    // Service target resolver: overrides base URL for configured providers
    let configs_for_endpoint = configs.clone();
    let target_resolver = genai::resolver::ServiceTargetResolver::from_resolver_fn(
        move |mut target: genai::ServiceTarget| {
            let provider = provider_for_adapter(target.model.adapter_kind);
            let result: std::result::Result<genai::ServiceTarget, genai::resolver::Error> =
                if let Some(cfg) = configs_for_endpoint.get(&provider) {
                    if let Some(ref base_url) = cfg.base_url {
                        if !base_url.is_empty() {
                            let arc_str: std::sync::Arc<str> = std::sync::Arc::from(base_url.as_str());
                            target.endpoint = genai::resolver::Endpoint::from_owned(arc_str);
                        }
                    }
                    if let Some(ref headers) = cfg.headers {
                        // TODO: genai's ServiceTarget doesn't expose header injection
                        // per-target yet. For now, custom headers are unsupported.
                        let _ = headers;
                    }
                    Ok(target)
                } else {
                    Ok(target)
                };
            result
        },
    );

    genai::Client::builder()
        .with_auth_resolver(auth_resolver)
        .with_service_target_resolver(target_resolver)
        .build()
}

/// Inverse of adapter_kind: returns the canonical provider name for an AdapterKind.
fn provider_for_adapter(kind: genai::adapter::AdapterKind) -> String {
    match kind {
        genai::adapter::AdapterKind::OpenAI => "openai".to_string(),
        genai::adapter::AdapterKind::Anthropic => "anthropic".to_string(),
        genai::adapter::AdapterKind::Gemini => "gemini".to_string(),
        genai::adapter::AdapterKind::Ollama => "ollama".to_string(),
        genai::adapter::AdapterKind::Groq => "groq".to_string(),
        genai::adapter::AdapterKind::DeepSeek => "deepseek".to_string(),
        genai::adapter::AdapterKind::Cohere => "cohere".to_string(),
        genai::adapter::AdapterKind::Xai => "xai".to_string(),
        // For any unknown adapter, use lowercase name
        other => format!("{other:?}").to_lowercase(),
    }
}

struct ExecState {
    client: genai::Client,
    tool_defs: Vec<piko_protocol::tools::ToolDef>,
}

pub struct LlmdExecutor {
    state: Arc<ExecState>,
    middlewares: Vec<Arc<dyn crate::middleware::LlmdMiddleware>>,
}

impl Default for LlmdExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmdExecutor {
    pub fn new() -> Self {
        Self {
            state: Arc::new(ExecState {
                client: genai::Client::default(),
                tool_defs: vec![],
            }),
            middlewares: vec![],
        }
    }

    pub fn from_providers(providers: HashMap<String, ProviderConfig>) -> Self {
        Self {
            state: Arc::new(ExecState {
                client: build_genai_client(&providers),
                tool_defs: vec![],
            }),
            middlewares: vec![],
        }
    }

    pub fn add_middleware(mut self, mw: Arc<dyn crate::middleware::LlmdMiddleware>) -> Self {
        self.middlewares.push(mw);
        self
    }
}

#[async_trait]
impl LlmGateway for LlmdExecutor {
    async fn chat_stream(
        &self,
        req: GatewayRequest,
        cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            return Err("cancelled".into());
        }

        let kind = adapter_kind(&req.provider);
        let model_iden = genai::ModelIden::new(kind, &req.model);
        let llm_messages = build_genai_messages(&req.system_prompt, &req.transcript);

        let mut request = genai::chat::ChatRequest::new(llm_messages);
        if !req.tools.is_empty() {
            let tools: Vec<genai::chat::Tool> =
                req.tools.iter().map(orch_tool_to_genai).collect();
            request = request.with_tools(tools);
        }

        let mut ctx = crate::middleware::GatewayContext {
            run_id: req.run_id.clone(),
            step_id: req.step_id.clone(),
            model_id: req.model.clone(),
            provider: req.provider.clone(),
            metadata: HashMap::new(),
        };

        // Pre-chat hooks
        for mw in self.middlewares.iter() {
            mw.pre_chat(&mut ctx, &mut request).await?;
        }

        // Apply resolved thinking level
        let chat_options = if let Some(ref thinking) = req.thinking {
            let effort = match thinking.as_str() {
                "none" => genai::chat::ReasoningEffort::None,
                "minimal" => genai::chat::ReasoningEffort::Minimal,
                "low" => genai::chat::ReasoningEffort::Low,
                "medium" => genai::chat::ReasoningEffort::Medium,
                "high" => genai::chat::ReasoningEffort::High,
                "xhigh" => genai::chat::ReasoningEffort::XHigh,
                "max" => genai::chat::ReasoningEffort::Max,
                other => {
                    // Try to parse as budget tokens
                    if let Ok(budget) = other.parse::<u32>() {
                        genai::chat::ReasoningEffort::Budget(budget)
                    } else {
                        genai::chat::ReasoningEffort::Medium
                    }
                }
            };
            Some(genai::chat::ChatOptions::default().with_reasoning_effort(effort))
        } else {
            None
        };

        // Open stream from genai — model first, request second, options third
        let chat_response = self
            .state
            .client
            .exec_chat_stream(model_iden, request, chat_options.as_ref())
            .await
            .map_err(|e| e.to_string())?;

        let mut llm_stream = chat_response.stream;
        let mut tool_counter: usize = 0;
        let middlewares = self.middlewares.clone();

        let stream = async_stream::stream! {
            while let Some(chunk_res) = llm_stream.next().await {
                if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
                    yield GatewayEvent::Done("abort".into());
                    return;
                }

                let mut gw_event = match chunk_res {
                    Ok(event) => match event {
                        genai::chat::ChatStreamEvent::Start => continue,
                        genai::chat::ChatStreamEvent::Chunk(chunk) => {
                            GatewayEvent::ContentDelta(chunk.content)
                        }
                        genai::chat::ChatStreamEvent::ReasoningChunk(chunk) => {
                            GatewayEvent::ReasoningDelta(chunk.content)
                        }
                        genai::chat::ChatStreamEvent::ToolCallChunk(chunk) => {
                            let tc = chunk.tool_call;
                            let index = tool_counter;
                            tool_counter += 1;
                            GatewayEvent::ToolCallStart {
                                index,
                                id: tc.call_id,
                                name: tc.fn_name,
                                args: tc.fn_arguments,
                            }
                        }
                        genai::chat::ChatStreamEvent::ThoughtSignatureChunk(_) => continue,
                        genai::chat::ChatStreamEvent::End(end) => {
                            if let Some(u) = end.captured_usage {
                                let mut usage = Usage::empty();
                                usage.input = u.prompt_tokens.unwrap_or(0) as u64;
                                usage.output = u.completion_tokens.unwrap_or(0) as u64;
                                // Cache tokens may be in prompt_tokens_details
                                usage.cache_read = u
                                    .prompt_tokens_details
                                    .as_ref()
                                    .and_then(|d| d.cached_tokens)
                                    .unwrap_or(0) as u64;
                                usage.cache_write = u
                                    .prompt_tokens_details
                                    .as_ref()
                                    .and_then(|d| d.cache_creation_tokens)
                                    .unwrap_or(0) as u64;
                                usage.total_tokens = usage.input + usage.output;
                                yield GatewayEvent::Usage(usage);
                            }
                            GatewayEvent::Done(
                                end.captured_stop_reason
                                    .map(|r| format!("{r:?}"))
                                    .unwrap_or_else(|| "stop".to_string()),
                            )
                        }
                    },
                    Err(e) => {
                        yield GatewayEvent::Error(e.to_string());
                        return;
                    }
                };

                for mw in middlewares.iter().rev() {
                    if let Err(e) = mw.on_stream_event(&mut ctx, &mut gw_event).await {
                        yield GatewayEvent::Error(e);
                        return;
                    }
                }

                yield gw_event;
            }
        };

        Ok(Box::pin(stream))
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
                .map(|t| piko_protocol::model::ToolInfo {
                    name: t.name.clone(),
                    description: t.description.clone(),
                })
                .collect(),
        }
    }

    async fn llm_call(
        &self,
        model: piko_protocol::messages::Model,
        system_prompt: Option<String>,
        messages: Vec<piko_protocol::messages::Message>,
        _settings: piko_protocol::model::ModelRunSettings,
    ) -> Result<String, String> {
        let kind = adapter_kind(&model.provider);
        let model_iden = genai::ModelIden::new(kind, &model.id);
        let sys = system_prompt.unwrap_or_default();
        let genai_messages = build_genai_messages(&sys, &messages);
        let request = genai::chat::ChatRequest::new(genai_messages);

        let resp = self
            .state
            .client
            .exec_chat(model_iden, request, None)
            .await
            .map_err(|e| e.to_string())?;

        Ok(resp
            .content
            .into_texts()
            .join("\n"))
    }
}

// ---- Tool conversion ----

fn orch_tool_to_genai(tool: &piko_protocol::tools::ToolDef) -> genai::chat::Tool {
    genai::chat::Tool::new(&tool.name)
        .with_description(tool.description.clone())
        .with_schema(tool.input_schema.clone())
}

// ---- Message conversion ----

fn build_genai_messages(
    system_prompt: &str,
    transcript: &[piko_protocol::messages::Message],
) -> Vec<genai::chat::ChatMessage> {
    let mut messages: Vec<genai::chat::ChatMessage> = Vec::with_capacity(transcript.len() + 1);

    if !system_prompt.is_empty() {
        messages.push(genai::chat::ChatMessage::system(system_prompt));
    }

    for msg in transcript {
        let genai_msg = match msg {
            piko_protocol::messages::Message::User { content, .. } => {
                let text = extract_text(content);
                genai::chat::ChatMessage::user(text)
            }
            piko_protocol::messages::Message::Assistant { content, .. } => {
                build_assistant_message(content)
            }
            piko_protocol::messages::Message::ToolResult {
                tool_call_id,
                content,
                ..
            } => {
                let text = extract_blocks(content);
                let content = genai::chat::MessageContent::from_parts(vec![
                    genai::chat::ContentPart::ToolResponse(
                        genai::chat::ToolResponse::new(tool_call_id.clone(), text),
                    ),
                ]);
                genai::chat::ChatMessage::new(genai::chat::ChatRole::Tool, content)
            }
        };
        messages.push(genai_msg);
    }

    messages
}

fn build_assistant_message(content: &[ContentBlock]) -> genai::chat::ChatMessage {
    let mut parts: Vec<genai::chat::ContentPart> = Vec::with_capacity(content.len());

    for block in content {
        match block {
            ContentBlock::Text { text } => {
                parts.push(genai::chat::ContentPart::Text(text.clone()));
            }
            ContentBlock::Thinking { thinking, .. } => {
                parts.push(genai::chat::ContentPart::ThoughtSignature(thinking.clone()));
            }
            ContentBlock::ToolCall {
                id,
                name,
                arguments,
                ..
            } => {
                parts.push(genai::chat::ContentPart::ToolCall(
                    genai::chat::ToolCall {
                        call_id: id.clone(),
                        fn_name: name.clone(),
                        fn_arguments: arguments.clone(),
                        thought_signatures: None,
                    },
                ));
            }
            ContentBlock::Image { .. } => {}
        }
    }

    genai::chat::ChatMessage::assistant(parts)
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
