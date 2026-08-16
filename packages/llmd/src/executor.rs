use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use piko_protocol::config::RetryConfig;
use piko_protocol::{
    TrajectoryFallback, TrajectoryIdentity, TrajectoryModelStepRecord, TrajectoryRetryAttempt,
    Usage,
};
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
        );
        let step_started_at = now_ms();
        let step_identity = TrajectoryIdentity {
            session_id: request.context.session_id.clone(),
            agent_instance_id: request.context.agent_instance_id.clone(),
            run_id: request.context.run_id.clone(),
            execution_id: None,
            source_turn_id: None,
        };
        let step_capture = ModelStepCapture {
            telemetry: Arc::clone(&self.telemetry),
            identity: step_identity,
            step_id: request.context.step_id.clone(),
            provider: request.model.provider.clone(),
            model: request.model.model.clone(),
            request: crate::redaction::semantic_model_input(&request),
            options: crate::redaction::semantic_inference_options(&request.options),
            started_at: step_started_at,
            message_id: request.context.step_message_id.clone(),
        };
        self.telemetry.record_model_step(TrajectoryModelStepRecord {
            identity: step_capture.identity.clone(),
            step_id: step_capture.step_id.clone(),
            provider: step_capture.provider.clone(),
            model: step_capture.model.clone(),
            request: step_capture.request.clone(),
            options: step_capture.options.clone(),
            started_at: step_capture.started_at,
            finished_at: None,
            duration_ms: None,
            retries: Vec::new(),
            fallback: None,
            response: None,
            message_id: Some(request.context.step_message_id.clone()),
            usage: None,
        });
        let state = Arc::clone(&self.state);
        let middlewares = self.middlewares.clone();
        let telemetry = Arc::clone(&self.telemetry);
        let policy = RetryPolicy::from_config(&self.retry);
        let mut retry_state = RetryState::default();
        let mut retries: Vec<TrajectoryRetryAttempt> = Vec::new();
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
                        Some(&mut retries),
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
        let mut fallback_info: Option<TrajectoryFallback> = None;
        let (initial_response, fallback) = match initial {
            Ok(response) => (Some(response), None),
            Err(error) if policy.enabled && target.streaming_fallback && error.is_retryable() => {
                telemetry.record_fallback(&target.model, &target.id);
                fallback_info = Some(TrajectoryFallback {
                    from_provider: target.id.clone(),
                    from_model: target.model.clone(),
                    to_provider: target.id.clone(),
                    to_model: target.model.clone(),
                    reason: "streaming fallback".into(),
                    at: now_ms(),
                });
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
        let output = wrap_model_step_finish(output, step_capture, retries, fallback_info);
        Ok(InferenceExecution {
            events: Box::pin(InstrumentedStream::new(Box::pin(output), span)),
            handle: None,
        })
    }
}

/// Per-step capture context shared between the live start record and the
/// finish record.
struct ModelStepCapture {
    telemetry: Arc<dyn crate::telemetry::GatewayTelemetry>,
    identity: TrajectoryIdentity,
    step_id: String,
    provider: String,
    model: String,
    request: serde_json::Value,
    options: serde_json::Value,
    started_at: i64,
    message_id: String,
}

/// Forward the step stream and record the finished model-step record once the
/// stream ends. If the consumer abandons the stream, no finish record is
/// written and the step stays "started" (interrupted).
fn wrap_model_step_finish(
    output: impl futures_core::Stream<Item = InferenceEvent> + Send + 'static,
    capture: ModelStepCapture,
    retries: Vec<TrajectoryRetryAttempt>,
    fallback: Option<TrajectoryFallback>,
) -> impl futures_core::Stream<Item = InferenceEvent> + Send + 'static {
    stream! {
        let mut usage: Option<Usage> = None;
        let mut inner = Box::pin(output);
        while let Some(event) = inner.next().await {
            if let InferenceEvent::Usage(value) = &event {
                usage = Some(value.clone());
            }
            yield event;
        }
        let finished_at = now_ms();
        capture.telemetry.record_model_step(TrajectoryModelStepRecord {
            identity: capture.identity,
            step_id: capture.step_id,
            provider: capture.provider,
            model: capture.model,
            request: capture.request,
            options: capture.options,
            started_at: capture.started_at,
            finished_at: Some(finished_at),
            duration_ms: Some(finished_at.saturating_sub(capture.started_at) as u64),
            retries,
            fallback,
            response: None,
            message_id: Some(capture.message_id),
            usage: usage.map(Box::new),
        });
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures::stream;
    use piko_protocol::Usage;

    use super::*;

    #[derive(Default)]
    struct RecordingTelemetry {
        finished: Mutex<Option<TrajectoryModelStepRecord>>,
    }

    impl crate::telemetry::GatewayTelemetry for RecordingTelemetry {
        fn record_model_step(&self, record: piko_protocol::TrajectoryModelStepRecord) {
            *self.finished.lock().unwrap() = Some(record);
        }

        fn record_ttft(&self, _model: &str, _provider: &str, _ttft_ms: u64) {}

        fn record_usage(&self, _model: &str, _provider: &str, _usage: &Usage) {}

        fn record_retry(&self, _model: &str, _provider: &str, _error_class: &str, _attempt: u32) {}

        fn record_fallback(&self, _model: &str, _provider: &str) {}
    }

    fn capture(telemetry: Arc<RecordingTelemetry>) -> ModelStepCapture {
        ModelStepCapture {
            telemetry: telemetry as Arc<dyn crate::telemetry::GatewayTelemetry>,
            identity: TrajectoryIdentity {
                session_id: "s".into(),
                agent_instance_id: "a".into(),
                run_id: "r".into(),
                execution_id: None,
                source_turn_id: None,
            },
            step_id: "step-1".into(),
            provider: "test".into(),
            model: "test-model".into(),
            request: serde_json::json!({}),
            options: serde_json::json!({}),
            started_at: 1,
            message_id: "message-1".into(),
        }
    }

    #[tokio::test]
    async fn finish_record_carries_final_step_usage() {
        let telemetry = Arc::new(RecordingTelemetry::default());
        let usage = Usage {
            input: 100,
            output: 20,
            cache_read: 80,
            cache_write: 20,
            total_tokens: 120,
            units: Default::default(),
            cost: Default::default(),
        };
        let input = stream::iter([
            InferenceEvent::Usage(usage.clone()),
            InferenceEvent::Completed(FinishReason::Completed {
                reason: "end_turn".into(),
            }),
        ]);
        let mut wrapped = Box::pin(wrap_model_step_finish(
            input,
            capture(Arc::clone(&telemetry)),
            Vec::new(),
            None,
        ));
        while wrapped.next().await.is_some() {}

        let record = telemetry
            .finished
            .lock()
            .unwrap()
            .clone()
            .expect("finish record written");
        assert_eq!(record.usage.as_deref(), Some(&usage));
        assert_eq!(record.message_id.as_deref(), Some("message-1"));
        assert!(record.finished_at.is_some());
        assert_eq!(record.step_id, "step-1");
    }

    #[tokio::test]
    async fn abandoned_stream_writes_no_finish_record() {
        let telemetry = Arc::new(RecordingTelemetry::default());
        let input = stream::iter([InferenceEvent::Usage(Usage {
            input: 10,
            output: 0,
            cache_read: 0,
            cache_write: 10,
            total_tokens: 10,
            units: Default::default(),
            cost: Default::default(),
        })]);
        let wrapped =
            wrap_model_step_finish(input, capture(Arc::clone(&telemetry)), Vec::new(), None);
        drop(wrapped);

        assert!(telemetry.finished.lock().unwrap().is_none());
    }
}
