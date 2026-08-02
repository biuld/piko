//! Resilient streaming: open-phase retry/backoff with a shared budget,
//! status-error peeking, and non-streaming fallback.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures::StreamExt;
use futures_core::Stream;
use tokio_util::sync::CancellationToken;
use tracing::{Level, event};

use crate::gateway::GatewayEvent;
use crate::middleware::{GatewayContext, LlmdMiddleware};
use crate::retry::{RetryPolicy, RetryState};
use crate::telemetry::GatewayTelemetry;

type ChatChunk = Result<genai::chat::ChatStreamEvent, genai::Error>;

/// Shared retry state for one request's open phase.
pub struct OpenRetryContext<'a> {
    pub policy: &'a RetryPolicy,
    pub state: &'a mut RetryState,
    pub cancel: Option<&'a CancellationToken>,
    pub run_id: &'a str,
    pub model: &'a str,
    pub provider: &'a str,
    pub telemetry: Arc<dyn GatewayTelemetry>,
}

/// Why the eager open phase failed.
pub enum OpenError {
    /// Not retryable (auth, bad request) or cancelled: fail immediately.
    NonRetryable(String),
    /// Retryable failures exhausted the attempt/backoff budget: the caller
    /// may try the non-streaming fallback.
    BudgetExhausted(String),
}

/// Outcome of the eager open phase in `chat_stream`.
pub enum OpenOutcome {
    /// A streaming request opened successfully; `Start` was already consumed
    /// and the first real event is re-injected at the head of the stream.
    Stream(Pin<Box<dyn Stream<Item = ChatChunk> + Send>>),
    /// The eager open failed after retries and the non-streaming fallback
    /// succeeded; events are already derived from the full response.
    FallbackEvents(Vec<GatewayEvent>),
}

/// Open a streaming request and retry retryable failures with capped,
/// jittered backoff within the shared budget. Cancellation aborts the sleep.
///
/// genai synthesizes a `Start` event on the first poll and defers HTTP status
/// checks to the second poll, so both polls happen here: status/transport
/// failures surface as open-phase failures and get the full retry/backoff
/// treatment before any content event reaches the caller.
pub async fn open_stream_with_retry(
    client: &genai::Client,
    model_iden: &genai::ModelIden,
    request: &genai::chat::ChatRequest,
    options: Option<&genai::chat::ChatOptions>,
    mut retry: OpenRetryContext<'_>,
) -> Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>, OpenError> {
    loop {
        if retry.cancel.is_some_and(|c| c.is_cancelled()) {
            return Err(OpenError::NonRetryable("cancelled".into()));
        }

        match client
            .exec_chat_stream(model_iden.clone(), request.clone(), options)
            .await
        {
            Ok(resp) => match peek_open_events(resp).await {
                Ok(stream) => return Ok(stream),
                Err(e) => handle_open_failure(e, &mut retry).await?,
            },
            Err(e) => handle_open_failure(e, &mut retry).await?,
        }
    }
}

/// Consume the synthesized `Start` event and the next event so status errors
/// are caught before the caller sees any content; re-inject the first real
/// event at the head of the returned stream.
async fn peek_open_events(
    resp: genai::chat::ChatStreamResponse,
) -> Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>, genai::Error> {
    let mut stream = resp.stream;
    match stream.next().await {
        // genai always synthesizes Start on the first poll.
        Some(Ok(genai::chat::ChatStreamEvent::Start)) => match stream.next().await {
            Some(Ok(first)) => Ok(prefixed(first, stream)),
            Some(Err(e)) => Err(e),
            None => Err(genai::Error::Internal(
                "stream ended before a terminal event".into(),
            )),
        },
        Some(Ok(first)) => Ok(prefixed(first, stream)),
        Some(Err(e)) => Err(e),
        None => Err(genai::Error::Internal(
            "stream ended before a terminal event".into(),
        )),
    }
}

fn prefixed(
    first: genai::chat::ChatStreamEvent,
    rest: genai::chat::ChatStream,
) -> Pin<Box<dyn Stream<Item = ChatChunk> + Send>> {
    Box::pin(futures::stream::iter([Ok(first)]).chain(rest))
}

/// Classify an open failure and either sleep (cancellable) and continue, or
/// return the error when it is not retryable or the budget is exhausted.
async fn handle_open_failure(
    error: genai::Error,
    retry: &mut OpenRetryContext<'_>,
) -> Result<(), OpenError> {
    let error_msg = error.to_string();
    let error_class = crate::retry::classify(&error).as_str();
    let error_truncated = truncate(&error_msg, 512);
    if !crate::retry::is_retryable(&error) {
        return Err(OpenError::NonRetryable(error_msg));
    }
    let Some(delay) = retry.policy.delay_for_retry(
        retry.state.retries_used,
        retry.state.elapsed_ms,
        crate::retry::jitter(),
    ) else {
        tracing::error!(
            target: "llm.retry_budget_exhausted",
            run_id = %retry.run_id,
            model = %retry.model,
            provider = %retry.provider,
            attempts = retry.state.retries_used + 1,
            error_class = error_class,
            error = %error_truncated,
            "llm.retry_budget_exhausted"
        );
        return Err(OpenError::BudgetExhausted(error_msg));
    };
    tracing::warn!(
        target: "llm.retry",
        run_id = %retry.run_id,
        model = %retry.model,
        provider = %retry.provider,
        attempt = retry.state.retries_used + 1,
        delay_ms = delay,
        error_class = error_class,
        error = %error_truncated,
        "llm.retry"
    );
    retry.telemetry.record_retry(
        retry.model,
        retry.provider,
        error_class,
        retry.state.retries_used + 1,
    );
    if let Some(cancel) = retry.cancel {
        tokio::select! {
            _ = cancel.cancelled() => return Err(OpenError::NonRetryable("cancelled".into())),
            _ = tokio::time::sleep(Duration::from_millis(delay)) => {}
        }
    } else {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    retry.state.record(delay);
    Ok(())
}

/// Build the gateway event stream for an opened request. Deltas map 1:1 from
/// the provider stream; a mid-stream failure surfaces as `GatewayEvent::Error`
/// (consumers own commit boundaries and never receive silently restarted
/// content).
pub fn resilient_stream(
    initial: OpenOutcome,
    cancel: Option<CancellationToken>,
    middlewares: Vec<Arc<dyn LlmdMiddleware>>,
    ctx: GatewayContext,
) -> Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>> {
    Box::pin(stream! {
        let mut ctx = ctx;
        let started = std::time::Instant::now();
        let mut first_content_received = false;
        let mut stream = match initial {
            OpenOutcome::Stream(stream) => stream,
            OpenOutcome::FallbackEvents(events) => {
                event!(
                    target: "llm.fallback",
                    Level::INFO,
                    run_id = %ctx.run_id,
                    step_id = %ctx.step_id,
                    model = %ctx.model_id,
                    provider = %ctx.provider,
                    "llm.fallback"
                );
                for mut event in events {
                    if let Err(e) = run_middleware(&middlewares, &mut ctx, &mut event).await {
                        yield GatewayEvent::Error(e);
                        return;
                    }
                    yield event;
                }
                return;
            }
        };

        while let Some(chunk_res) = stream.next().await {
            if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
                yield GatewayEvent::Done("abort".into());
                return;
            }

            let mut event = match chunk_res {
                Ok(genai::chat::ChatStreamEvent::Start) => continue,
                Ok(genai::chat::ChatStreamEvent::Chunk(chunk)) => {
                    if !first_content_received {
                        first_content_received = true;
                        let ttft_ms = started.elapsed().as_millis() as u64;
                        event!(
                            target: "llm.ttft",
                            Level::INFO,
                            run_id = %ctx.run_id,
                            step_id = %ctx.step_id,
                            model = %ctx.model_id,
                            provider = %ctx.provider,
                            ttft_ms,
                            "llm.ttft"
                        );
                        ctx.telemetry().record_ttft(&ctx.model_id, &ctx.provider, ttft_ms);
                    }
                    GatewayEvent::ContentDelta(chunk.content)
                }
                Ok(genai::chat::ChatStreamEvent::ReasoningChunk(chunk)) => {
                    GatewayEvent::ReasoningDelta(chunk.content)
                }
                Ok(genai::chat::ChatStreamEvent::ToolCallChunk(chunk)) => {
                    let tc = chunk.tool_call;
                    let args_delta = if let serde_json::Value::String(s) = tc.fn_arguments {
                        s
                    } else {
                        serde_json::to_string(&tc.fn_arguments).unwrap_or_default()
                    };
                    GatewayEvent::ToolCallChunk {
                        id: tc.call_id,
                        name: tc.fn_name,
                        args_delta,
                    }
                }
                Ok(genai::chat::ChatStreamEvent::ThoughtSignatureChunk(_)) => continue,
                Ok(genai::chat::ChatStreamEvent::End(end)) => {
                    if let Some(u) = end.captured_usage {
                        let mut usage_event = GatewayEvent::Usage(usage_from_genai(&u));
                        if let Err(e) = run_middleware(&middlewares, &mut ctx, &mut usage_event).await
                        {
                            yield GatewayEvent::Error(e);
                            return;
                        }
                        yield usage_event;
                    }
                    GatewayEvent::Done(
                        end.captured_stop_reason
                            .as_ref()
                            .map(stop_reason_string)
                            .unwrap_or_else(|| "stop".to_string()),
                    )
                }
                Err(e) => {
                    let error_class = crate::retry::classify(&e).as_str();
                    event!(
                        target: "llm.stream_error",
                        Level::ERROR,
                        run_id = %ctx.run_id,
                        step_id = %ctx.step_id,
                        model = %ctx.model_id,
                        provider = %ctx.provider,
                        error_class,
                        error = %truncate(&e.to_string(), 512),
                        "llm.stream_error"
                    );
                    yield GatewayEvent::Error(e.to_string());
                    return;
                }
            };

            let is_done = matches!(event, GatewayEvent::Done(_));
            if let GatewayEvent::Done(reason) = &event {
                event!(
                    target: "llm.stream_done",
                    Level::INFO,
                    run_id = %ctx.run_id,
                    step_id = %ctx.step_id,
                    model = %ctx.model_id,
                    provider = %ctx.provider,
                    reason = %reason,
                    "llm.stream_done"
                );
            }
            if let Err(e) = run_middleware(&middlewares, &mut ctx, &mut event).await {
                yield GatewayEvent::Error(e);
                return;
            }
            yield event;

            if is_done {
                return;
            }
        }

        // The stream ended without a terminal event: surface it as an error.
        event!(
            target: "llm.stream_error",
            Level::ERROR,
            run_id = %ctx.run_id,
            step_id = %ctx.step_id,
            model = %ctx.model_id,
            provider = %ctx.provider,
            error_class = "stream_parse",
            error = "stream ended before a terminal event",
            "llm.stream_error"
        );
        yield GatewayEvent::Error("stream ended before a terminal event".into());
    })
}

/// Bound error bodies recorded in span events/metrics to a sane size.
fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        let mut result = text.chars().take(max).collect::<String>();
        result.push_str("...");
        result
    }
}

/// Wrap a stream so every poll runs inside `span`, attaching the events and
/// child spans emitted while consuming the model stream to it.
pub struct InstrumentedStream<S> {
    inner: S,
    span: tracing::Span,
}

impl<S> InstrumentedStream<S> {
    pub fn new(inner: S, span: tracing::Span) -> Self {
        Self { inner, span }
    }
}

impl<S> Stream for InstrumentedStream<S>
where
    S: Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let span = &this.span;
        let inner = Pin::new(&mut this.inner);
        span.in_scope(|| inner.poll_next(cx))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

async fn run_middleware(
    middlewares: &[Arc<dyn LlmdMiddleware>],
    ctx: &mut GatewayContext,
    event: &mut GatewayEvent,
) -> Result<(), String> {
    for mw in middlewares.iter().rev() {
        mw.on_stream_event(ctx, event).await?;
    }
    Ok(())
}

/// Derive the standard gateway event sequence from a non-streaming response.
pub fn fallback_events(resp: genai::chat::ChatResponse) -> Vec<GatewayEvent> {
    let mut events = Vec::new();
    if let Some(reasoning) = resp.reasoning_content {
        events.push(GatewayEvent::ReasoningDelta(reasoning));
    }
    for text in resp.content.texts() {
        events.push(GatewayEvent::ContentDelta(text.to_string()));
    }
    for tc in resp.content.tool_calls() {
        let args_delta = if let serde_json::Value::String(s) = &tc.fn_arguments {
            s.clone()
        } else {
            serde_json::to_string(&tc.fn_arguments).unwrap_or_default()
        };
        events.push(GatewayEvent::ToolCallChunk {
            id: tc.call_id.clone(),
            name: tc.fn_name.clone(),
            args_delta,
        });
    }
    events.push(GatewayEvent::Usage(usage_from_genai(&resp.usage)));
    events.push(GatewayEvent::Done(
        resp.stop_reason
            .as_ref()
            .map(stop_reason_string)
            .unwrap_or_else(|| "stop".to_string()),
    ));
    events
}

pub fn usage_from_genai(u: &genai::chat::Usage) -> piko_protocol::messages::Usage {
    let mut usage = piko_protocol::messages::Usage::empty();
    usage.input = u.prompt_tokens.unwrap_or(0) as u64;
    usage.output = u.completion_tokens.unwrap_or(0) as u64;
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
    usage
}

pub fn stop_reason_string(reason: &genai::chat::StopReason) -> String {
    match reason {
        genai::chat::StopReason::Completed(_) | genai::chat::StopReason::StopSequence(_) => {
            "stop".to_string()
        }
        genai::chat::StopReason::ToolCall(_) => "tool_use".to_string(),
        genai::chat::StopReason::MaxTokens(_) => "length".to_string(),
        genai::chat::StopReason::ContentFilter(s) | genai::chat::StopReason::Other(s) => s.clone(),
    }
}
