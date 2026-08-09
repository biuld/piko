use std::sync::Arc;
use std::time::Duration;

use piko_protocol::config::RetryConfig;
use tokio_util::sync::CancellationToken;

use crate::gateway::{
    ErrorClass, GatewayError, ModelEvent, ModelRequest, ModelResult, SemanticItem,
};
use crate::middleware::{GatewayContext, LlmdMiddleware};
use crate::modeling::ProtocolKind;
use crate::protocols::ProtocolAdapter;
use crate::protocols::chat_completions::ChatCompletionsAdapter;
use crate::protocols::responses::ResponsesAdapter;
use crate::retry::{RetryPolicy, RetryState};
use crate::target::ModelTarget;

pub(super) fn adapter(protocol: ProtocolKind) -> Box<dyn ProtocolAdapter> {
    match protocol {
        ProtocolKind::Responses => Box::new(ResponsesAdapter),
        ProtocolKind::ChatCompletions => Box::new(ChatCompletionsAdapter),
    }
}

pub(super) async fn execute_fallback(
    client: &reqwest::Client,
    request: &ModelRequest,
    target: &ModelTarget,
    cancel: &CancellationToken,
) -> Result<ModelResult, GatewayError> {
    let adapter = adapter(target.protocol.kind());
    let body = adapter.encode(request, target, false)?;
    let response = crate::transport::send(client, target, &body, false, cancel).await?;
    let value = crate::transport::json(response, target, cancel).await?;
    adapter.decode_response(value, target)
}

pub(super) async fn execute_once_with_retry(
    client: &reqwest::Client,
    request: &ModelRequest,
    target: &ModelTarget,
    cancel: &CancellationToken,
    retry: &RetryConfig,
    telemetry: &Arc<dyn crate::telemetry::GatewayTelemetry>,
) -> Result<ModelResult, GatewayError> {
    let policy = RetryPolicy::from_config(retry);
    let mut state = RetryState::default();
    loop {
        match execute_fallback(client, request, target, cancel).await {
            Ok(result) => return Ok(result),
            Err(error) => {
                if !retry_or_sleep(&policy, &mut state, &error, cancel, telemetry, target).await {
                    if cancel.is_cancelled() {
                        return Err(GatewayError::cancelled(&target.id));
                    }
                    return Err(error);
                }
            }
        }
    }
}

pub(super) async fn retry_or_sleep(
    policy: &RetryPolicy,
    state: &mut RetryState,
    error: &GatewayError,
    cancel: &CancellationToken,
    telemetry: &Arc<dyn crate::telemetry::GatewayTelemetry>,
    target: &ModelTarget,
) -> bool {
    if !error.is_retryable() {
        return false;
    }
    let Some(mut delay) =
        policy.delay_for_retry(state.retries_used, state.elapsed_ms, crate::retry::jitter())
    else {
        return false;
    };
    if let Some(retry_after) = error.retry_after_ms {
        delay = delay.max(retry_after);
    }
    if state.elapsed_ms.saturating_add(delay) > policy.budget_ms {
        return false;
    }
    telemetry.record_retry(
        &target.model,
        &target.id,
        error_class(error.class),
        state.retries_used + 1,
    );
    tracing::warn!(
        target: "llm.retry",
        attempt = state.retries_used + 1,
        delay_ms = delay,
        error_class = error_class(error.class),
        "llm.retry"
    );
    let slept = tokio::select! {
        _ = cancel.cancelled() => false,
        _ = tokio::time::sleep(Duration::from_millis(delay)) => true,
    };
    if slept {
        state.record(delay);
    }
    slept
}

fn error_class(class: ErrorClass) -> &'static str {
    match class {
        ErrorClass::Target => "target",
        ErrorClass::UnsupportedCapability => "unsupported_capability",
        ErrorClass::Authentication => "authentication",
        ErrorClass::Transport => "transport",
        ErrorClass::Timeout => "timeout",
        ErrorClass::RateLimit => "rate_limit",
        ErrorClass::Upstream => "upstream",
        ErrorClass::Sse => "sse",
        ErrorClass::Protocol => "protocol",
        ErrorClass::Cancelled => "cancelled",
    }
}

pub(super) async fn apply_middleware(
    event: ModelEvent,
    middlewares: &[Arc<dyn LlmdMiddleware>],
    context: &mut GatewayContext,
) -> ModelEvent {
    let mut event = event;
    for middleware in middlewares {
        if let Err(message) = middleware.on_stream_event(context, &mut event).await {
            return ModelEvent::Error(GatewayError::new(
                ErrorClass::Upstream,
                &context.provider,
                "middleware",
                message,
            ));
        }
    }
    event
}

pub(super) fn is_observable(event: &ModelEvent) -> bool {
    matches!(
        event,
        ModelEvent::TextDelta { .. }
            | ModelEvent::RefusalDelta { .. }
            | ModelEvent::ReasoningDelta { .. }
            | ModelEvent::FunctionCallDelta { .. }
    )
}

pub(super) fn result_events(result: ModelResult) -> Vec<ModelEvent> {
    let mut events = Vec::new();
    for item in result.items {
        events.push(match item {
            SemanticItem::Text { text, identity } => ModelEvent::TextDelta {
                delta: text,
                identity,
            },
            SemanticItem::Refusal { text, identity } => ModelEvent::RefusalDelta {
                delta: text,
                identity,
            },
            SemanticItem::Reasoning { text, identity } => ModelEvent::ReasoningDelta {
                delta: text,
                identity,
            },
            SemanticItem::FunctionCall {
                name,
                arguments,
                identity,
            } => ModelEvent::FunctionCallDelta {
                name,
                arguments_delta: arguments,
                identity,
            },
            SemanticItem::FunctionResult { .. } => continue,
        });
    }
    if let Some(usage) = result.usage {
        events.push(ModelEvent::Usage(usage));
    }
    events.push(ModelEvent::OutputMetadata(result.output_metadata));
    events.push(ModelEvent::Completed(result.status));
    events
}

pub(super) struct InstrumentedStream<S> {
    inner: S,
    span: tracing::Span,
}

impl<S> InstrumentedStream<S> {
    pub(super) fn new(inner: S, span: tracing::Span) -> Self {
        Self { inner, span }
    }
}

impl<S> futures::Stream for InstrumentedStream<S>
where
    S: futures::Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.span
            .in_scope(|| std::pin::Pin::new(&mut this.inner).poll_next(context))
    }
}
