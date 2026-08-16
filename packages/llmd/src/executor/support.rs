use std::sync::Arc;
use std::time::Duration;

use piko_protocol::TrajectoryRetryAttempt;
use tokio_util::sync::CancellationToken;

use crate::gateway::{
    ErrorClass, InferenceError, InferenceEvent, InferenceItem, InferenceRequest, InferenceResult,
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
    request: &InferenceRequest,
    target: &ModelTarget,
    cancel: &CancellationToken,
) -> Result<InferenceResult, InferenceError> {
    let adapter = adapter(target.protocol.kind());
    let plan = crate::checkpoint::plan(target, &request.conversation)?;
    let body = adapter.encode(request, target, &plan, false)?;
    let response = crate::transport::send(client, target, &body, false, cancel).await?;
    let value = crate::transport::json(response, target, cancel).await?;
    adapter.decode_response(value, target, request)
}

pub(super) async fn retry_or_sleep(
    policy: &RetryPolicy,
    state: &mut RetryState,
    error: &InferenceError,
    cancel: &CancellationToken,
    telemetry: &Arc<dyn crate::telemetry::GatewayTelemetry>,
    target: &ModelTarget,
    trajectory_retries: Option<&mut Vec<TrajectoryRetryAttempt>>,
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
    if let Some(retries) = trajectory_retries {
        retries.push(TrajectoryRetryAttempt {
            attempt: state.retries_used + 1,
            delay_ms: delay,
            error: crate::redaction::sanitize_diagnostic(&error.to_string()),
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        });
    }
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
        ErrorClass::CheckpointRejected => "checkpoint_rejected",
        ErrorClass::ContinuationUnavailable => "continuation_unavailable",
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
    event: InferenceEvent,
    middlewares: &[Arc<dyn LlmdMiddleware>],
    context: &mut GatewayContext,
) -> InferenceEvent {
    let mut event = event;
    for middleware in middlewares {
        if let Err(message) = middleware.on_stream_event(context, &mut event).await {
            return InferenceEvent::Error(InferenceError::new(
                ErrorClass::Upstream,
                &context.provider,
                "middleware",
                message,
            ));
        }
    }
    event
}

pub(super) fn is_observable(event: &InferenceEvent) -> bool {
    matches!(
        event,
        InferenceEvent::TextDelta { .. }
            | InferenceEvent::RefusalDelta { .. }
            | InferenceEvent::ReasoningDelta { .. }
            | InferenceEvent::ToolCallDelta { .. }
    )
}

pub(super) fn result_events(result: InferenceResult) -> Vec<InferenceEvent> {
    let mut events = Vec::new();
    for item in result.items {
        events.push(match item {
            InferenceItem::Text { text, id } => InferenceEvent::TextDelta {
                delta: text,
                item_id: id,
            },
            InferenceItem::Refusal { text, id } => InferenceEvent::RefusalDelta {
                delta: text,
                item_id: id,
            },
            InferenceItem::Reasoning { text, id } => InferenceEvent::ReasoningDelta {
                delta: text,
                item_id: id,
            },
            InferenceItem::ToolCall {
                name,
                arguments,
                call_id,
            } => InferenceEvent::ToolCallDelta {
                name,
                arguments_delta: arguments,
                call_id,
            },
            InferenceItem::ToolResult { .. } => continue,
        });
    }
    events.extend(result.auxiliary.into_iter().map(|item| match item {
        crate::tools::InferenceAuxiliary::UpstreamActivity(value) => {
            InferenceEvent::UpstreamActivity(value)
        }
        crate::tools::InferenceAuxiliary::ApprovalRequired(value) => {
            InferenceEvent::ApprovalRequired(value)
        }
        crate::tools::InferenceAuxiliary::Source(value) => InferenceEvent::Source(value),
        crate::tools::InferenceAuxiliary::Citation(value) => InferenceEvent::Citation(value),
        crate::tools::InferenceAuxiliary::Artifact(value) => InferenceEvent::Artifact(value),
    }));
    if let Some(usage) = result.usage {
        events.push(InferenceEvent::Usage(usage));
    }
    if let Some(checkpoint) = result.checkpoint {
        events.push(InferenceEvent::Checkpoint(checkpoint));
    }
    events.push(InferenceEvent::Completed(result.finish_reason));
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
