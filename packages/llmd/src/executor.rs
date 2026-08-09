use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use piko_protocol::config::RetryConfig;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::gateway::{
    ErrorClass, GatewayError, LlmGateway, ModelEvent, ModelEventStream, ModelRequest, ModelResult,
    SemanticItem, TerminalStatus,
};
use crate::middleware::{GatewayContext, LlmdMiddleware};
use crate::retry::{RetryPolicy, RetryState};
use crate::target::{ModelTarget, ModelTargetConfig};

mod support;
use support::*;

struct ExecState {
    client: reqwest::Client,
    targets: HashMap<String, ModelTargetConfig>,
    auth_resolver: Option<Arc<dyn crate::providers::RuntimeAuthResolver>>,
}

pub struct LlmdExecutor {
    state: Arc<ExecState>,
    middlewares: Vec<Arc<dyn LlmdMiddleware>>,
    retry: RetryConfig,
    telemetry: Arc<dyn crate::telemetry::GatewayTelemetry>,
}

impl Default for LlmdExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmdExecutor {
    pub fn new() -> Self {
        Self::from_targets(HashMap::new())
    }

    pub fn from_targets(targets: HashMap<String, ModelTargetConfig>) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(10 * 60))
            .build()
            .expect("reqwest client configuration is valid");
        Self {
            state: Arc::new(ExecState {
                client,
                targets,
                auth_resolver: None,
            }),
            middlewares: Vec::new(),
            retry: RetryConfig::default(),
            telemetry: Arc::new(crate::telemetry::NoopGatewayTelemetry),
        }
    }

    pub fn with_auth_resolver(
        mut self,
        resolver: Arc<dyn crate::providers::RuntimeAuthResolver>,
    ) -> Self {
        Arc::get_mut(&mut self.state)
            .expect("configure auth before sharing")
            .auth_resolver = Some(resolver);
        self
    }

    pub fn with_telemetry(
        mut self,
        telemetry: Arc<dyn crate::telemetry::GatewayTelemetry>,
    ) -> Self {
        self.telemetry = telemetry;
        self
    }

    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    pub fn add_middleware(mut self, middleware: Arc<dyn LlmdMiddleware>) -> Self {
        self.middlewares.push(middleware);
        self
    }

    async fn target(&self, request: &ModelRequest) -> Result<ModelTarget, GatewayError> {
        let model_key = crate::modeling::ModelKey::new(&request.provider, &request.model);
        let lookup_id = model_key.lookup_id();
        let config = self.state.targets.get(&lookup_id).ok_or_else(|| {
            GatewayError::new(
                ErrorClass::Target,
                &lookup_id,
                "resolve_target",
                "model target is not configured",
            )
        })?;
        let auth = if let Some(resolver) = &self.state.auth_resolver {
            resolver
                .resolve(&request.provider, config.auth_method)
                .await
                .map_err(|message| {
                    GatewayError::new(
                        ErrorClass::Authentication,
                        &request.provider,
                        "resolve_auth",
                        message,
                    )
                })?
        } else {
            None
        };
        ModelTarget::resolve(
            &lookup_id,
            &request.model,
            config,
            auth.as_ref().map(|value| &value.headers),
        )
    }

    fn context(&self, request: &ModelRequest) -> GatewayContext {
        GatewayContext {
            run_id: request.run_id.clone(),
            step_id: request.step_id.clone(),
            model_id: request.model.clone(),
            provider: request.provider.clone(),
            metadata: HashMap::new(),
            telemetry: Some(Arc::clone(&self.telemetry)),
        }
    }
}

#[async_trait]
impl LlmGateway for LlmdExecutor {
    async fn execute(
        &self,
        mut request: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelEventStream, GatewayError> {
        if cancel.is_cancelled() {
            return Err(GatewayError::cancelled(&request.provider));
        }
        let target = self.target(&request).await?;
        target.validate(&request)?;
        let mut context = self.context(&request);
        for middleware in &self.middlewares {
            middleware
                .pre_execute(&mut context, &mut request)
                .await
                .map_err(|message| {
                    GatewayError::new(ErrorClass::Target, &target.id, "middleware", message)
                })?;
        }
        let protocol_adapter = adapter(target.protocol.kind());
        let body = protocol_adapter.encode(&request, &target, true)?;
        self.telemetry
            .record_model_input(piko_protocol::ModelInputDebugSnapshot {
                session_id: request.session_id.clone(),
                agent_instance_id: request.agent_instance_id.clone(),
                run_id: request.run_id.clone(),
                step_id: request.step_id.clone(),
                provider: request.provider.clone(),
                model: request.model.clone(),
                request: body.clone(),
                options: serde_json::json!({
                    "protocol": target.protocol.kind(),
                    "streamingFallback": target.streaming_fallback
                }),
            });

        let span = tracing::info_span!(
            "llm.request",
            run_id = %request.run_id,
            step_id = %request.step_id,
            model = %request.model,
            provider = %request.provider,
            protocol = ?target.protocol.kind(),
            streaming = true,
        );
        let state = Arc::clone(&self.state);
        let middlewares = self.middlewares.clone();
        let telemetry = Arc::clone(&self.telemetry);
        let policy = RetryPolicy::from_config(&self.retry);
        let mut retry_state = RetryState::default();
        let initial = loop {
            match crate::transport::send(&state.client, &target, &body, true, &cancel)
                .instrument(span.clone())
                .await
            {
                Ok(response) => break Ok(response),
                Err(error) => {
                    if retry_or_sleep(
                        &policy,
                        &mut retry_state,
                        &error,
                        &cancel,
                        &telemetry,
                        &target,
                    )
                    .instrument(span.clone())
                    .await
                    {
                        continue;
                    }
                    if cancel.is_cancelled() {
                        break Err(GatewayError::cancelled(&target.id));
                    }
                    break Err(error);
                }
            }
        };
        let (initial_response, fallback) = match initial {
            Ok(response) => (Some(response), None),
            Err(error) if policy.enabled && target.streaming_fallback && error.is_retryable() => {
                telemetry.record_fallback(&target.model, &target.id);
                let result = execute_fallback(&state.client, &request, &target, &cancel)
                    .instrument(span.clone())
                    .await?;
                (None, Some(result))
            }
            Err(error) => return Err(error),
        };

        let output = stream! {
            let started = Instant::now();
            let mut context = context;
            let mut protocol_stream = adapter(target.protocol.kind()).new_stream(&target);
            let mut first_output = false;
            let mut pending_events = Vec::new();
            if let Some(result) = fallback {
                for event in result_events(result) {
                    let event = apply_middleware(event, &middlewares, &mut context).await;
                    if matches!(event, ModelEvent::Completed(_)) {
                        tracing::info!(target: "llm.stream_done", "llm.stream_done");
                    }
                    yield event;
                }
                return;
            }
            let Some(response) = initial_response else { return };
            let mut sse = crate::transport::sse(response, target.id.clone(), cancel.clone());
                        let mut stream_error = None;
                        while let Some(message) = sse.next().await {
                            match message {
                                Ok(message) if message.done => {
                                    for event in pending_events.drain(..) {
                                        yield apply_middleware(event, &middlewares, &mut context).await;
                                    }
                                    match protocol_stream.finish() {
                                        Ok(events) => for event in events {
                                            let event = apply_middleware(event, &middlewares, &mut context).await;
                                            if matches!(event, ModelEvent::Completed(_)) {
                                                tracing::info!(target: "llm.stream_done", "llm.stream_done");
                                            }
                                            yield event;
                                        },
                                        Err(error) => yield ModelEvent::Error(error),
                                    }
                                    return;
                                }
                                Ok(message) => {
                                    let value = match serde_json::from_str(&message.data) {
                                        Ok(value) => value,
                                        Err(error) => {
                                            yield ModelEvent::Error(GatewayError::new(
                                                ErrorClass::Protocol, &target.id, "decode_stream_json",
                                                format!("malformed stream JSON: {error}"),
                                            ));
                                            return;
                                        }
                                    };
                                    match protocol_stream.push(value) {
                                        Ok(events) => for event in events {
                                            if !first_output {
                                                if is_observable(&event) {
                                                    first_output = true;
                                                    let ttft_ms = started.elapsed().as_millis() as u64;
                                                    tracing::info!(target: "llm.ttft", ttft_ms, "llm.ttft");
                                                    telemetry.record_ttft(&target.model, &target.id, ttft_ms);
                                                    for pending in pending_events.drain(..) {
                                                        yield apply_middleware(
                                                            pending,
                                                            &middlewares,
                                                            &mut context,
                                                        )
                                                        .await;
                                                    }
                                                } else {
                                                    pending_events.push(event);
                                                    continue;
                                                }
                                            }
                                            yield apply_middleware(event, &middlewares, &mut context).await;
                                        },
                                        Err(error) => { yield ModelEvent::Error(error); return; }
                                    }
                                }
                                Err(error) => { stream_error = Some(error); break; }
                            }
                        }
                        match protocol_stream.finish() {
                            Ok(events) => {
                                for event in pending_events.drain(..) {
                                    yield apply_middleware(event, &middlewares, &mut context).await;
                                }
                                for event in events {
                                    let event = apply_middleware(event, &middlewares, &mut context).await;
                                    if matches!(event, ModelEvent::Completed(_)) {
                                        tracing::info!(target: "llm.stream_done", "llm.stream_done");
                                    }
                                    yield event;
                                }
                                return;
                            }
                            Err(protocol_error) => {
                                let retryable_stream_failure = stream_error
                                    .as_ref()
                                    .is_some_and(GatewayError::is_retryable);
                                let error = stream_error.unwrap_or(protocol_error);
                                if error.class == ErrorClass::Cancelled {
                                    yield ModelEvent::Completed(TerminalStatus::Cancelled);
                                    return;
                                }
                                if protocol_stream.has_observable_output() {
                                    yield ModelEvent::Error(error);
                                    return;
                                }
                                if policy.enabled
                                    && target.streaming_fallback
                                    && retryable_stream_failure
                                {
                                    telemetry.record_fallback(&target.model, &target.id);
                                    match execute_fallback(
                                        &state.client,
                                        &request,
                                        &target,
                                        &cancel,
                                    )
                                    .await
                                    {
                                        Ok(result) => {
                                            for event in result_events(result) {
                                                yield apply_middleware(
                                                    event,
                                                    &middlewares,
                                                    &mut context,
                                                )
                                                .await;
                                            }
                                        }
                                        Err(error) => yield ModelEvent::Error(error),
                                    }
                                    return;
                                }
                                // A protocol-level EOF or framing failure is
                                // never silently restarted or reinterpreted.
                                yield ModelEvent::Error(error);
                                return;
                            }
                        }
        };
        Ok(Box::pin(InstrumentedStream::new(Box::pin(output), span)))
    }

    async fn execute_once(
        &self,
        mut request: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelResult, GatewayError> {
        let target = self.target(&request).await?;
        let mut context = self.context(&request);
        for middleware in &self.middlewares {
            middleware
                .pre_execute(&mut context, &mut request)
                .await
                .map_err(|message| {
                    GatewayError::new(ErrorClass::Target, &target.id, "middleware", message)
                })?;
        }
        let mut result = execute_once_with_retry(
            &self.state.client,
            &request,
            &target,
            &cancel,
            &self.retry,
            &self.telemetry,
        )
        .await?;
        if let Some(usage) = result.usage.take() {
            let event =
                apply_middleware(ModelEvent::Usage(usage), &self.middlewares, &mut context).await;
            if let ModelEvent::Usage(usage) = event {
                result.usage = Some(usage);
            }
        }
        Ok(result)
    }

    async fn llm_call(
        &self,
        model: piko_protocol::Model,
        system_prompt: Option<String>,
        messages: Vec<piko_protocol::Message>,
        _settings: piko_protocol::model::ModelRunSettings,
    ) -> Result<String, String> {
        let mut prompt = piko_protocol::SemanticRunPrompt::default();
        if let Some(content) = system_prompt {
            prompt.blocks.push(piko_protocol::PromptBlock {
                id: "stateless.system".into(),
                kind: piko_protocol::PromptBlockKind::Instruction,
                authority: piko_protocol::InstructionAuthority::Platform,
                trust: piko_protocol::ContentTrust::Trusted,
                source: piko_protocol::PromptSource::new("stateless", "llm_call"),
                content,
                content_digest: String::new(),
                cache_scope: piko_protocol::CacheScope::NoCache,
            });
        }
        let result = self
            .execute_once(
                ModelRequest {
                    session_id: "stateless".into(),
                    agent_instance_id: "stateless".into(),
                    provider: model.provider,
                    model: model.id,
                    run_prompt: prompt,
                    transcript: messages,
                    tools: Vec::new(),
                    run_id: "stateless".into(),
                    step_id: "stateless".into(),
                    thinking: None,
                },
                CancellationToken::new(),
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(result
            .items
            .into_iter()
            .filter_map(|item| match item {
                SemanticItem::Text { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }
}
