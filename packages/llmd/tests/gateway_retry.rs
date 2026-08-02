//! Integration tests for the model-gateway retry/backoff budget, mid-stream
//! restart, and non-streaming fallback, driven against a local stub HTTP
//! server (no external network).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use piko_llmd::executor::LlmdExecutor;
use piko_llmd::gateway::{GatewayEvent, GatewayRequest, LlmGateway};
use piko_protocol::config::{ProviderConfig, RetryConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// ---- Stub HTTP server ----

#[derive(Debug, Clone)]
struct RequestInfo {
    streaming: bool,
}

#[derive(Debug, Clone)]
enum Step {
    Status(u16),
    StreamSuccess,
    StreamPartialThenClose,
    NonStreaming,
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
        Step::NonStreaming => {
            let body = r#"{"id":"chatcmpl-1","object":"chat.completion","created":0,"model":"gpt-test","choices":[{"index":0,"message":{"role":"assistant","content":"fallback text"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
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
    let mut providers = HashMap::new();
    providers.insert(
        "openai".to_string(),
        ProviderConfig {
            kind: "openai".to_string(),
            api_key: "test".to_string(),
            base_url: Some(format!("http://{addr}")),
            headers: None,
            streaming_fallback,
        },
    );
    LlmdExecutor::from_providers(providers).with_retry(retry_config())
}

fn request() -> GatewayRequest {
    GatewayRequest {
        provider: "openai".to_string(),
        model: "gpt-test".to_string(),
        run_prompt: piko_protocol::SemanticRunPrompt::default(),
        transcript: vec![piko_protocol::messages::Message::User {
            content: piko_protocol::messages::MessageContent::String("hi".to_string()),
            timestamp: None,
        }],
        tools: vec![],
        run_id: "run-1".to_string(),
        step_id: "step-1".to_string(),
        thinking: None,
    }
}

async fn collect(exec: &LlmdExecutor, req: GatewayRequest) -> Vec<GatewayEvent> {
    let stream = exec
        .chat_stream(req, None)
        .await
        .expect("chat_stream should return a stream");
    stream.collect::<Vec<_>>().await
}

fn text_events(events: &[GatewayEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            GatewayEvent::ContentDelta(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

// ---- Tests ----

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
            .any(|e| matches!(e, GatewayEvent::Done(r) if r == "stop"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GatewayEvent::Usage(u) if u.input == 3 && u.output == 2))
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
            .any(|e| matches!(e, GatewayEvent::Done(r) if r == "stop"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GatewayEvent::Usage(u) if u.input == 10 && u.output == 5))
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

#[tokio::test]
async fn fallback_disabled_fails_with_streaming_error() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(503)],
    })
    .await;
    let exec = executor(stub.addr, Some(false));

    let result = exec.chat_stream(request(), None).await;

    // The 503 status surfaces during the open phase; after the retry budget
    // the request fails closed without fallback.
    assert!(result.is_err(), "fallback disabled must fail closed");
    assert_eq!(stub.request_count(), 3, "retries still occur; no fallback");
    assert_eq!(stub.non_streaming_count(), 0);
}

#[tokio::test]
async fn mid_stream_break_surfaces_error_without_restart() {
    let stub = Stub::start(Script {
        steps: vec![Step::StreamPartialThenClose, Step::StreamSuccess],
    })
    .await;
    let exec = executor(stub.addr, None);

    let events = collect(&exec, request()).await;

    // A mid-stream break surfaces as an error; the gateway never restarts
    // after content has been delivered (consumers own commit boundaries), so
    // no second request is made.
    assert!(events.iter().any(|e| matches!(e, GatewayEvent::Error(_))));
    assert!(!events.iter().any(|e| matches!(e, GatewayEvent::Done(_))));
    assert_eq!(stub.request_count(), 1);
}

#[tokio::test]
async fn non_retryable_open_fails_immediately() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(401)],
    })
    .await;
    let exec = executor(stub.addr, None);

    let result = exec.chat_stream(request(), None).await;

    // genai surfaces the 401 as the first stream event; the open phase
    // classifies it as non-retryable and fails immediately.
    assert!(result.is_err());
    assert_eq!(stub.request_count(), 1, "no retries for 401");
}

#[tokio::test]
async fn llm_call_retries_transient_errors() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(503), Step::NonStreaming],
    })
    .await;
    let exec = executor(stub.addr, None);

    let model = piko_protocol::messages::Model {
        id: "gpt-test".to_string(),
        name: "gpt-test".to_string(),
        provider: "openai".to_string(),
        base_url: None,
    };
    let out = exec
        .llm_call(
            model,
            None,
            vec![],
            piko_protocol::model::ModelRunSettings::default(),
        )
        .await
        .expect("llm_call should retry and succeed");

    assert_eq!(out, "fallback text");
    assert_eq!(stub.request_count(), 2, "one failure then one success");
    assert_eq!(stub.non_streaming_count(), 2);
}
