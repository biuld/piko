use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use piko_protocol::config::{ProviderConfig, RetryConfig};
use piko_protocol::model::ModelCapabilities;

use crate::gateway::{GatewayEvent, GatewayRequest, LlmGateway};
use crate::retry::{RetryPolicy, RetryState};
use crate::stream::{
    OpenError, OpenOutcome, OpenRetryContext, open_stream_with_retry, resilient_stream,
};

mod prompt_mapping;
use prompt_mapping::{build_genai_messages, stateless_system_block};

// ---- genai-based executor ----

/// Maps a provider kind to a genai AdapterKind.
fn adapter_kind(provider: &str) -> genai::adapter::AdapterKind {
    match provider.to_lowercase().as_str() {
        "openai" | "azure" | "openrouter" => genai::adapter::AdapterKind::OpenAI,
        "groq" => genai::adapter::AdapterKind::Groq,
        "deepseek" => genai::adapter::AdapterKind::DeepSeek,
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
    let auth_resolver =
        genai::resolver::AuthResolver::from_resolver_fn(move |model_iden: genai::ModelIden| {
            let provider = provider_for_adapter(model_iden.adapter_kind);
            let result: std::result::Result<
                Option<genai::resolver::AuthData>,
                genai::resolver::Error,
            > = if let Some(cfg) = configs_for_auth.get(&provider) {
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
        });

    // Service target resolver: overrides base URL for configured providers
    let configs_for_endpoint = configs.clone();
    let target_resolver = genai::resolver::ServiceTargetResolver::from_resolver_fn(
        move |mut target: genai::ServiceTarget| {
            let provider = provider_for_adapter(target.model.adapter_kind);
            let result: std::result::Result<genai::ServiceTarget, genai::resolver::Error> =
                if let Some(cfg) = configs_for_endpoint.get(&provider) {
                    if let Some(ref base_url) = cfg.base_url
                        && !base_url.is_empty()
                    {
                        let arc_str: std::sync::Arc<str> = std::sync::Arc::from(base_url.as_str());
                        target.endpoint = genai::resolver::Endpoint::from_owned(arc_str);
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
    /// Per-provider streaming-fallback opt-out (default enabled).
    streaming_fallback: HashMap<String, bool>,
}

pub struct LlmdExecutor {
    state: Arc<ExecState>,
    middlewares: Vec<Arc<dyn crate::middleware::LlmdMiddleware>>,
    retry: RetryConfig,
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
                streaming_fallback: HashMap::new(),
            }),
            middlewares: vec![],
            retry: RetryConfig::default(),
        }
    }

    pub fn from_providers(providers: HashMap<String, ProviderConfig>) -> Self {
        let streaming_fallback = providers
            .iter()
            .map(|(id, cfg)| (id.clone(), cfg.streaming_fallback.unwrap_or(true)))
            .collect();
        Self {
            state: Arc::new(ExecState {
                client: build_genai_client(&providers),
                tool_defs: vec![],
                streaming_fallback,
            }),
            middlewares: vec![],
            retry: RetryConfig::default(),
        }
    }

    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
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
        let llm_messages = build_genai_messages(&req.run_prompt, &req.transcript);

        let mut request = genai::chat::ChatRequest::new(llm_messages);
        if !req.tools.is_empty() {
            let tools: Vec<genai::chat::Tool> = req.tools.iter().map(orch_tool_to_genai).collect();
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
        let mut chat_options = genai::chat::ChatOptions::default().with_capture_usage(true);
        if let Some(ref thinking) = req.thinking {
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
            chat_options = chat_options.with_reasoning_effort(effort);
        }
        use piko_protocol::PromptCachePolicy;
        match req.run_prompt.cache_plan.policy {
            PromptCachePolicy::Disabled => {}
            PromptCachePolicy::ProviderDefault => {
                chat_options = chat_options.with_prompt_cache_key(provider_cache_key(&req));
            }
            PromptCachePolicy::Ephemeral => {
                chat_options = chat_options
                    .with_prompt_cache_key(provider_cache_key(&req))
                    .with_cache_control(genai::chat::CacheControl::Ephemeral);
            }
            PromptCachePolicy::Extended => {
                chat_options = chat_options
                    .with_prompt_cache_key(provider_cache_key(&req))
                    .with_cache_control(genai::chat::CacheControl::Ephemeral24h);
            }
        }
        let chat_options = Some(chat_options);

        // Eager open phase: retry with the shared budget, then fall back to a
        // non-streaming completion before returning the stream.
        let policy = RetryPolicy::from_config(&self.retry);
        let fallback_enabled = self
            .state
            .streaming_fallback
            .get(&req.provider)
            .copied()
            .unwrap_or(true);
        let client = self.state.client.clone();
        let mut retry_state = RetryState::default();

        let retry_ctx = OpenRetryContext {
            policy: &policy,
            state: &mut retry_state,
            cancel: cancel.as_ref(),
            run_id: &req.run_id,
        };
        let initial = match open_stream_with_retry(
            &client,
            &model_iden,
            &request,
            chat_options.as_ref(),
            retry_ctx,
        )
        .await
        {
            Ok(resp) => OpenOutcome::Stream(resp),
            Err(open_err) => match open_err {
                OpenError::NonRetryable(msg) => return Err(msg),
                OpenError::BudgetExhausted(msg) => {
                    if fallback_enabled {
                        match client
                            .exec_chat(model_iden.clone(), request.clone(), chat_options.as_ref())
                            .await
                        {
                            Ok(resp) => {
                                OpenOutcome::FallbackEvents(crate::stream::fallback_events(resp))
                            }
                            Err(fb_err) => {
                                return Err(format!(
                                    "streaming request failed after retries: {msg}; \
                                     non-streaming fallback failed: {fb_err}"
                                ));
                            }
                        }
                    } else {
                        return Err(msg);
                    }
                }
            },
        };

        Ok(resilient_stream(
            initial,
            cancel,
            self.middlewares.clone(),
            ctx,
        ))
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
        let prompt = piko_protocol::SemanticRunPrompt {
            blocks: if sys.is_empty() {
                Vec::new()
            } else {
                vec![stateless_system_block(sys)]
            },
            ..Default::default()
        };
        let genai_messages = build_genai_messages(&prompt, &messages);
        let request = genai::chat::ChatRequest::new(genai_messages);

        let policy = RetryPolicy::from_config(&self.retry);
        let mut state = RetryState::default();
        let resp = loop {
            match self
                .state
                .client
                .exec_chat(model_iden.clone(), request.clone(), None)
                .await
            {
                Ok(resp) => break resp,
                Err(e) => {
                    if !crate::retry::is_retryable(&e) {
                        return Err(e.to_string());
                    }
                    let Some(delay) = policy.delay_for_retry(
                        state.retries_used,
                        state.elapsed_ms,
                        crate::retry::jitter(),
                    ) else {
                        return Err(e.to_string());
                    };
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    state.record(delay);
                }
            }
        };

        Ok(resp.content.into_texts().join("\n"))
    }
}

// ---- Tool conversion ----

fn orch_tool_to_genai(tool: &piko_protocol::tools::ToolDef) -> genai::chat::Tool {
    genai::chat::Tool::new(&tool.name)
        .with_description(tool.description.clone())
        .with_schema(tool.input_schema.clone())
}

fn provider_cache_key(request: &GatewayRequest) -> String {
    format!(
        "piko-prompt-map-v1:{}:{}:assembly-v{}:{}",
        request.provider,
        request.model,
        request.run_prompt.assembly_version,
        request.run_prompt.cache_plan.semantic_prefix_digest
    )
}
