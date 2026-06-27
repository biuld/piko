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

// ---- self-llm based executor ----

struct ExecState {
    provider_configs: HashMap<String, ProviderConfig>,
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
                provider_configs: HashMap::new(),
                tool_defs: vec![],
            }),
            middlewares: vec![],
        }
    }

    pub fn from_providers(providers: HashMap<String, ProviderConfig>) -> Self {
        Self {
            state: Arc::new(ExecState {
                provider_configs: providers,
                tool_defs: vec![],
            }),
            middlewares: vec![],
        }
    }

    pub fn add_middleware(mut self, mw: Arc<dyn crate::middleware::LlmdMiddleware>) -> Self {
        self.middlewares.push(mw);
        self
    }

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
            _ => self_llm::config::ProviderType::OpenAi,
        };

        let base_url = config
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

#[async_trait]
impl LlmGateway for LlmdExecutor {
    async fn chat_stream(
        &self,
        req: GatewayRequest,
        cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        // Check cancellation before any work
        if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            return Err("cancelled".into());
        }

        let client = self.build_client(&req.provider)?;

        let llm_messages = build_llm_messages(&req.system_prompt, &req.transcript);

        let mut request = self_llm::ChatRequest::new(&req.model, llm_messages);
        if !req.tools.is_empty() {
            let llm_tools: Vec<self_llm::Tool> =
                req.tools.iter().map(orch_tool_to_self_llm).collect();
            request = request.tools(llm_tools);
        }

        let mut ctx = crate::middleware::GatewayContext {
            run_id: req.run_id.clone(),
            step_id: req.step_id.clone(),
            model_id: req.model.clone(),
            provider: req.provider.clone(),
            metadata: HashMap::new(),
        };

        // 1. Execute pre_chat hooks (forward, async)
        for mw in self.middlewares.iter() {
            mw.pre_chat(&mut ctx, &mut request).await?;
        }

        // 2. Open the stream from self_llm (async — this is where the HTTP connection starts)
        let mut llm_stream = client
            .chat_stream(request)
            .await
            .map_err(|e| e.to_string())?;

        // 3. Build a transformed stream: each chunk passes through middlewares and cancellation check
        let middlewares = self.middlewares.clone();
        let stream = async_stream::stream! {
            while let Some(chunk_res) = llm_stream.next().await {
                // Cancellation check
                if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
                    yield GatewayEvent::Done("abort".into());
                    return;
                }

                let mut gw_event = match chunk_res {
                    Ok(event) => match event {
                        self_llm::types::StreamEvent::ContentDelta(text) => {
                            GatewayEvent::ContentDelta(text)
                        }
                        self_llm::types::StreamEvent::ReasoningDelta(text) => {
                            GatewayEvent::ReasoningDelta(text)
                        }
                        self_llm::types::StreamEvent::ToolCallStart { index, id, name } => {
                            GatewayEvent::ToolCallStart { index, id, name }
                        }
                        self_llm::types::StreamEvent::ToolCallDelta {
                            index,
                            arguments_delta,
                        } => GatewayEvent::ToolCallDelta {
                            index,
                            arguments_delta,
                        },
                        self_llm::types::StreamEvent::Usage(u) => {
                            let mut usage = Usage::empty();
                            usage.input = u.input_tokens as u64;
                            usage.output = u.output_tokens as u64;
                            usage.cache_read =
                                u.cache_read_input_tokens.unwrap_or(0) as u64;
                            usage.cache_write =
                                u.cache_creation_input_tokens.unwrap_or(0) as u64;
                            usage.total_tokens = usage.input + usage.output;
                            GatewayEvent::Usage(usage)
                        }
                        self_llm::types::StreamEvent::Done(reason) => {
                            GatewayEvent::Done(format!("{reason:?}"))
                        }
                    },
                    Err(e) => {
                        yield GatewayEvent::Error(e.to_string());
                        return;
                    }
                };

                // 4. Post-commit hooks (reverse order) — each middleware gets a chance to modify the event
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
        let client = self
            .build_client(&model.provider)
            .map_err(|e| e.to_string())?;

        let sys = system_prompt.unwrap_or_default();
        let llm_messages = build_llm_messages(&sys, &messages);
        let request = self_llm::ChatRequest::new(&model.id, llm_messages);

        let resp = client
            .chat(request)
            .await
            .map_err(|e| e.to_string())?;
        let text = resp.text().unwrap_or_default().to_string();
        Ok(text)
    }
}

fn orch_tool_to_self_llm(tool: &piko_protocol::tools::ToolDef) -> self_llm::Tool {
    self_llm::Tool {
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters: tool.input_schema.clone(),
    }
}

fn build_llm_messages(
    system_prompt: &str,
    transcript: &[piko_protocol::messages::Message],
) -> Vec<self_llm::Message> {
    let mut messages: Vec<self_llm::Message> = Vec::with_capacity(transcript.len() + 1);

    if !system_prompt.is_empty() {
        messages.push(self_llm::Message::system(system_prompt));
    }

    for msg in transcript {
        let llm_msg = match msg {
            piko_protocol::messages::Message::User { content, .. } => {
                let text = extract_text(content);
                self_llm::Message::user(text)
            }
            piko_protocol::messages::Message::Assistant { content, .. } => {
                build_assistant_message(content)
            }
            piko_protocol::messages::Message::ToolResult {
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
