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
    ErrorClass, FinishReason, InferenceError, InferenceEvent, InferenceExecution, InferenceGateway,
    InferenceRequest,
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

    async fn target_for(
        &self,
        model: &crate::gateway::ModelRef,
    ) -> Result<ModelTarget, InferenceError> {
        let model_key = crate::modeling::ModelKey::new(&model.provider, &model.model);
        let lookup_id = model_key.lookup_id();
        let config = self.state.targets.get(&lookup_id).ok_or_else(|| {
            InferenceError::new(
                ErrorClass::Target,
                &lookup_id,
                "resolve_target",
                "model target is not configured",
            )
        })?;
        let auth = if let Some(resolver) = &self.state.auth_resolver {
            resolver
                .resolve(&model.provider, config.auth_method)
                .await
                .map_err(|message| {
                    InferenceError::new(
                        ErrorClass::Authentication,
                        &model.provider,
                        "resolve_auth",
                        message,
                    )
                })?
        } else {
            None
        };
        ModelTarget::resolve(
            &lookup_id,
            &model.model,
            config,
            auth.as_ref().map(|value| &value.headers),
        )
    }

    async fn target(&self, request: &InferenceRequest) -> Result<ModelTarget, InferenceError> {
        self.target_for(&request.model).await
    }

    fn context(&self, request: &InferenceRequest, target: &ModelTarget) -> GatewayContext {
        GatewayContext {
            run_id: request.context.run_id.clone(),
            step_id: request.context.step_id.clone(),
            model_id: request.model.model.clone(),
            provider: request.model.provider.clone(),
            api_surface: target.api_surface.clone(),
            auth_method: Some(target.auth_method),
            billing: target.billing.clone(),
            metadata: HashMap::new(),
            telemetry: Some(Arc::clone(&self.telemetry)),
        }
    }
}

#[async_trait]
impl InferenceGateway for LlmdExecutor {
    async fn describe(
        &self,
        model: &crate::gateway::ModelRef,
    ) -> Result<crate::gateway::ModelDescriptor, InferenceError> {
        Ok(self.target_for(model).await?.descriptor(&model.provider))
    }

    async fn start(
        &self,
        mut request: InferenceRequest,
        cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        if cancel.is_cancelled() {
            return Err(InferenceError::cancelled(&request.model.provider));
        }
        let target = self.target(&request).await?;
        target.validate(&request)?;
        let mut context = self.context(&request, &target);
        for middleware in &self.middlewares {
            middleware
                .pre_execute(&mut context, &mut request)
                .await
                .map_err(|message| {
                    InferenceError::new(ErrorClass::Target, &target.id, "middleware", message)
                })?;
        }
        let protocol_adapter = adapter(target.protocol.kind());
        let plan = crate::checkpoint::plan(&target, &request.conversation)?;
        let body = protocol_adapter.encode(&request, &target, &plan, true)?;
        let content_attributes = self
            .telemetry
            .capture_content()
            .then(|| crate::genai_telemetry::content_attributes(&request));

        let span = tracing::info_span!(
            "llm.request",
            otel.kind = "client",
            session_id = %request.context.session_id,
            run_id = %request.context.run_id,
            agent_instance_id = %request.context.agent_instance_id,
            step_id = %request.context.step_id,
            model = %request.model.model,
            provider = %request.model.provider,
            protocol = ?target.protocol.kind(),
            streaming = true,
            "gen_ai.operation.name" = "chat",
            "gen_ai.provider.name" = %request.model.provider,
            "gen_ai.conversation.id" = %request.context.session_id,
            "gen_ai.request.model" = %request.model.model,
            "gen_ai.request.stream" = true,
            "gen_ai.request.max_tokens" = tracing::field::Empty,
            "gen_ai.request.reasoning.level" = tracing::field::Empty,
        );
        if let Some(max_tokens) = request.options.max_output_tokens {
            span.record("gen_ai.request.max_tokens", max_tokens as u64);
        }
        if let Some(reasoning) = request.options.reasoning_effort.as_ref() {
            span.record("gen_ai.request.reasoning.level", reasoning.as_str());
        }
        if let Some(content) = content_attributes {
            span.in_scope(|| self.telemetry.record_genai_content(&content));
        }
        self.telemetry
            .record_model_input(piko_protocol::ModelInputDebugSnapshot {
                session_id: request.context.session_id.clone(),
                agent_instance_id: request.context.agent_instance_id.clone(),
                run_id: request.context.run_id.clone(),
                step_id: request.context.step_id.clone(),
                provider: request.model.provider.clone(),
                model: request.model.model.clone(),
                request: crate::redaction::semantic_model_input(&request),
                options: crate::redaction::semantic_inference_options(&request.options),
            });
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
                        break Err(InferenceError::cancelled(&target.id));
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
            let mut protocol_stream = adapter(target.protocol.kind()).new_stream(&target, &request);
            let mut first_output = false;
            let mut pending_events = Vec::new();
            if let Some(result) = fallback {
                for event in result_events(result) {
                    let event = apply_middleware(event, &middlewares, &mut context).await;
                    if matches!(event, InferenceEvent::Completed(_)) {
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
                                            if matches!(event, InferenceEvent::Completed(_)) {
                                                tracing::info!(target: "llm.stream_done", "llm.stream_done");
                                            }
                                            yield event;
                                        },
                                        Err(error) => yield InferenceEvent::Error(error),
                                    }
                                    return;
                                }
                                Ok(message) => {
                                    let value = match serde_json::from_str(&message.data) {
                                        Ok(value) => value,
                                        Err(error) => {
                                            yield InferenceEvent::Error(InferenceError::new(
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
                                        Err(error) => { yield InferenceEvent::Error(error); return; }
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
                                    if matches!(event, InferenceEvent::Completed(_)) {
                                        tracing::info!(target: "llm.stream_done", "llm.stream_done");
                                    }
                                    yield event;
                                }
                                return;
                            }
                            Err(protocol_error) => {
                                let retryable_stream_failure = stream_error
                                    .as_ref()
                                    .is_some_and(InferenceError::is_retryable);
                                let error = stream_error.unwrap_or(protocol_error);
                                if error.class == ErrorClass::Cancelled {
                                    yield InferenceEvent::Completed(FinishReason::Cancelled);
                                    return;
                                }
                                if protocol_stream.has_observable_output() {
                                    yield InferenceEvent::Error(error);
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
                                        Err(error) => yield InferenceEvent::Error(error),
                                    }
                                    return;
                                }
                                // A protocol-level EOF or framing failure is
                                // never silently restarted or reinterpreted.
                                yield InferenceEvent::Error(error);
                                return;
                            }
                        }
        };
        Ok(InferenceExecution {
            events: Box::pin(InstrumentedStream::new(Box::pin(output), span)),
            handle: None,
        })
    }
}
