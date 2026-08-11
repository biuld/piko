//! Integration tests for the model-gateway retry/backoff budget, mid-stream
//! restart, and non-streaming fallback, driven against a local stub HTTP
//! server (no external network).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use piko_llmd::executor::LlmdExecutor;
use piko_llmd::gateway::{FinishReason, InferenceEvent, InferenceGateway, InferenceRequest};
use piko_llmd::target::ModelTargetConfig;
use piko_llmd::telemetry::GatewayTelemetry;
use piko_protocol::config::RetryConfig;
use piko_protocol::messages::Usage;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[path = "gateway_retry/cases.rs"]
mod cases;

// ---- Stub HTTP server ----

#[derive(Debug, Clone)]
struct RequestInfo {
    streaming: bool,
}

#[derive(Debug, Clone)]
enum Step {
    Status(u16),
    StreamSuccess,
    StreamBreakBeforeOutput,
    StreamPartialThenClose,
    NonStreaming,
    ResponsesStreamSuccess,
    ResponsesNonStreaming,
}

#[derive(Debug, Clone)]
struct Script {
    steps: Vec<Step>,
}

impl Script {
    fn step_for(&self, index: usize) -> Step {
        self.steps
            .get(index)
            .cloned()
            .unwrap_or_else(|| self.steps.last().cloned().expect("script cannot be empty"))
    }
}

struct Stub {
    _handle: tokio::task::JoinHandle<()>,
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<RequestInfo>>>,
}

impl Stub {
    async fn start(script: Script) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let counter = Arc::new(AtomicUsize::new(0));
        let requests_serve = requests.clone();
        let counter_serve = counter.clone();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let script = script.clone();
                let requests = requests_serve.clone();
                let counter = counter_serve.clone();
                tokio::spawn(async move {
                    handle_request(stream, &script, &requests, &counter).await;
                });
            }
        });
        Self {
            _handle: handle,
            addr,
            requests,
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    fn streaming_count(&self) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.streaming)
            .count()
    }

    fn non_streaming_count(&self) -> usize {
        let total = self.requests.lock().unwrap().len();
        total - self.streaming_count()
    }
}

async fn handle_request(
    mut stream: TcpStream,
    script: &Script,
    requests: &Mutex<Vec<RequestInfo>>,
    counter: &AtomicUsize,
) {
    let (headers, body) = read_request(&mut stream).await;
    let body_str = String::from_utf8_lossy(&body);
    let streaming = body_str.contains("\"stream\":true");
    let index = counter.fetch_add(1, Ordering::SeqCst);
    requests.lock().unwrap().push(RequestInfo { streaming });
    let step = script.step_for(index);
    let _ = headers;
    write_response(&mut stream, step).await;
}

async fn read_request(stream: &mut TcpStream) -> (HashMap<String, String>, Vec<u8>) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_sequence(&buf, b"\r\n\r\n") {
            let header_end = pos + 4;
            let header_text = String::from_utf8_lossy(&buf[..pos]);
            let mut headers = HashMap::new();
            for line in header_text.lines().skip(1) {
                if let Some((k, v)) = line.split_once(':') {
                    headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                }
            }
            let content_length: usize = headers
                .get("content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            while buf.len() < header_end + content_length {
                let n = stream.read(&mut tmp).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
            }
            let end = (header_end + content_length).min(buf.len());
            let body = buf[header_end..end].to_vec();
            return (headers, body);
        }
    }
    (HashMap::new(), buf)
}

fn find_sequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

async fn write_response(stream: &mut TcpStream, step: Step) {
    match step {
        Step::Status(code) => {
            let body = "upstream error";
            let head = format!(
                "HTTP/1.1 {code} Error\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        }
        Step::StreamSuccess => {
            let head =
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
            let body = concat!(
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
                "data: [DONE]\n\n",
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        }
        Step::StreamPartialThenClose => {
            let head =
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
            let partial = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"par\"},\"finish_reason\":null}]}\n\n";
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(partial.as_bytes()).await;
            // Drop the socket without a terminal event: the stream broke.
        }
        Step::StreamBreakBeforeOutput => {
            let head = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: text/event-stream\r\n",
                "Content-Length: 128\r\n",
                "Connection: close\r\n\r\n"
            );
            let _ = stream.write_all(head.as_bytes()).await;
            // Closing before the promised body length is a retryable transport
            // failure, and no semantic output has been observed.
        }
        Step::NonStreaming => {
            let body = r#"{"id":"chatcmpl-1","object":"chat.completion","created":0,"model":"gpt-test","choices":[{"index":0,"message":{"role":"assistant","content":"fallback text"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        }
        Step::ResponsesStreamSuccess => {
            let head =
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
            let body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n",
                "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"delta\":\"native\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"usage\":{\"input_tokens\":4,\"output_tokens\":1}}}\n\n",
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        }
        Step::ResponsesNonStreaming => {
            let body = r#"{"id":"resp_1","status":"completed","output":[{"type":"message","id":"msg_1","content":[{"type":"output_text","text":"native"}]}],"usage":{"input_tokens":4,"output_tokens":1}}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        }
    }
    let _ = stream.flush().await;
}

// ---- Fixtures ----

fn retry_config() -> RetryConfig {
    RetryConfig {
        enabled: true,
        max_retries: 2,
        base_delay_ms: 1,
        max_delay_ms: 10,
        budget_ms: 10_000,
    }
}

fn executor(addr: SocketAddr, streaming_fallback: Option<bool>) -> LlmdExecutor {
    executor_for_protocol(
        addr,
        streaming_fallback,
        piko_llmd::modeling::ProtocolProfile::ChatCompletions,
    )
}

fn executor_for_protocol(
    addr: SocketAddr,
    streaming_fallback: Option<bool>,
    protocol: piko_llmd::modeling::ProtocolProfile,
) -> LlmdExecutor {
    let mut targets = HashMap::new();
    let mut config = ModelTargetConfig::new(
        "openai/gpt-test@platform",
        "platform",
        piko_protocol::model::ProviderAuthMethod::ApiKey,
        protocol,
    );
    config.base_url = Some(format!("http://{addr}"));
    config.streaming_fallback = streaming_fallback.unwrap_or(true);
    config.pricing = Some(piko_llmd::modeling::TokenPricing {
        currency: "USD".into(),
        basis: piko_protocol::messages::UsageCostBasis::ListPrice,
        input_per_million: 2.5,
        cached_input_per_million: 1.25,
        output_per_million: 10.0,
        cache_write_per_million: None,
        tiers: Vec::new(),
    });
    targets.insert("openai/gpt-test".to_string(), config.clone());
    config.target_id = "openai/gpt-4o@platform".into();
    targets.insert("openai/gpt-4o".to_string(), config);
    LlmdExecutor::from_targets(targets).with_retry(retry_config())
}

fn request() -> InferenceRequest {
    InferenceRequest {
        model: piko_llmd::gateway::ModelRef::new("openai", "gpt-test"),
        conversation: piko_llmd::gateway::Conversation::from_messages(
            piko_protocol::SemanticRunPrompt::default(),
            vec![piko_protocol::messages::Message::User {
                content: piko_protocol::messages::MessageContent::String("hi".to_string()),
                timestamp: None,
            }],
        ),
        tools: vec![],
        options: Default::default(),
        context: piko_llmd::gateway::InvocationContext {
            session_id: "session-1".to_string(),
            agent_instance_id: "agent-1".to_string(),
            run_id: "run-1".to_string(),
            step_id: "step-1".to_string(),
        },
    }
}

#[derive(Default)]
struct PromptCapture(Mutex<Vec<piko_protocol::ModelInputDebugSnapshot>>);

impl GatewayTelemetry for PromptCapture {
    fn record_model_input(&self, input: piko_protocol::ModelInputDebugSnapshot) {
        self.0.lock().unwrap().push(input);
    }

    fn record_ttft(&self, _model: &str, _provider: &str, _ttft_ms: u64) {}
    fn record_usage(&self, _model: &str, _provider: &str, _usage: &Usage) {}
    fn record_retry(&self, _model: &str, _provider: &str, _error_class: &str, _attempt: u32) {}
    fn record_fallback(&self, _model: &str, _provider: &str) {}
}

async fn collect(exec: &LlmdExecutor, req: InferenceRequest) -> Vec<InferenceEvent> {
    let stream = exec
        .start(req, tokio_util::sync::CancellationToken::new())
        .await
        .expect("execute should return a stream");
    stream.events.collect::<Vec<_>>().await
}

fn text_events(events: &[InferenceEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            InferenceEvent::TextDelta { delta, .. } => Some(delta.clone()),
            _ => None,
        })
        .collect()
}

// ---- Tests ----

#[tokio::test]
async fn llm_request_span_records_retry_ttft_usage_and_done_events() {
    use opentelemetry_sdk::testing::trace::new_tokio_test_exporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing_subscriber::layer::SubscriberExt;

    let (span_exporter, mut span_rx, _shutdown_rx) = new_tokio_test_exporter();
    let tracer_provider = SdkTracerProvider::builder()
        .with_simple_exporter(span_exporter)
        .build();
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    let otel_layer = tracing_opentelemetry::layer()
        .with_tracer(opentelemetry::global::tracer("gateway_retry_otel_test"));
    let subscriber = tracing_subscriber::registry()
        .with(otel_layer)
        .with(tracing_subscriber::fmt::layer().with_ansi(false));
    static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INIT.get_or_init(|| {
        tracing::subscriber::set_global_default(subscriber).unwrap();
    });

    let stub = Stub::start(Script {
        steps: vec![Step::Status(503), Step::StreamSuccess],
    })
    .await;
    let mut targets = HashMap::new();
    let mut target = ModelTargetConfig::new(
        "openai/gpt-test@platform",
        "platform",
        piko_protocol::model::ProviderAuthMethod::ApiKey,
        piko_llmd::modeling::ProtocolProfile::ChatCompletions,
    );
    target.base_url = Some(format!("http://{}", stub.addr));
    targets.insert("openai/gpt-test".to_string(), target);
    let exec = piko_llmd::build_gateway(targets, retry_config());
    let mut req = request();
    req.context.run_id = "run-otel".to_string();
    let stream = exec
        .start(req, tokio_util::sync::CancellationToken::new())
        .await
        .expect("execute should return a stream");
    let events = stream.events.collect::<Vec<_>>().await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, InferenceEvent::Completed(FinishReason::Completed { reason }) if reason == "stop"))
    );

    tracer_provider.force_flush().unwrap();
    let mut spans = Vec::new();
    while let Ok(span) = span_rx.try_recv() {
        spans.push(span);
    }

    let llm_request = spans
        .iter()
        .find(|span| {
            span.name.as_ref() == "llm.request"
                && span
                    .attributes
                    .iter()
                    .any(|kv| kv.key.as_str() == "run_id" && kv.value.to_string() == "run-otel")
        })
        .unwrap_or_else(|| panic!("no llm.request span exported: {spans:?}"));

    let event_names: Vec<&str> = llm_request.events.iter().map(|e| e.name.as_ref()).collect();
    for expected in ["llm.retry", "llm.ttft", "llm.usage", "llm.stream_done"] {
        assert!(
            event_names.contains(&expected),
            "expected {expected:?} event on llm.request, got {event_names:?}"
        );
    }

    let attrs = |key: &str| {
        llm_request
            .attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.to_string())
    };
    let model = attrs("model");
    assert_eq!(model.as_deref(), Some("gpt-test"));
    let provider = attrs("provider");
    assert_eq!(provider.as_deref(), Some("openai"));
    let run_id = attrs("run_id");
    assert_eq!(run_id.as_deref(), Some("run-otel"));
}

#[tokio::test]
async fn retries_transient_503_then_streams() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(503), Step::Status(503), Step::StreamSuccess],
    })
    .await;
    let exec = executor(stub.addr, None);

    let events = collect(&exec, request()).await;

    assert_eq!(
        text_events(&events),
        vec!["Hel".to_string(), "lo".to_string()]
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, InferenceEvent::Completed(FinishReason::Completed { reason }) if reason == "stop"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, InferenceEvent::Usage(u) if u.input == 3 && u.output == 2))
    );
    assert_eq!(
        stub.request_count(),
        3,
        "one initial attempt plus two retries"
    );
    assert_eq!(stub.streaming_count(), 3);
}

#[tokio::test]
async fn falls_back_to_non_streaming_after_budget_exhausted() {
    let stub = Stub::start(Script {
        steps: vec![
            Step::Status(503),
            Step::Status(503),
            Step::Status(503),
            Step::NonStreaming,
        ],
    })
    .await;
    let exec = executor(stub.addr, None);

    let events = collect(&exec, request()).await;

    assert_eq!(text_events(&events), vec!["fallback text".to_string()]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, InferenceEvent::Completed(FinishReason::Completed { reason }) if reason == "stop"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, InferenceEvent::Usage(u) if u.input == 10 && u.output == 5))
    );
    assert_eq!(
        stub.streaming_count(),
        3,
        "initial attempt plus two retries"
    );
    assert_eq!(
        stub.non_streaming_count(),
        1,
        "exactly one fallback request"
    );
}
