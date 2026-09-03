//! Standalone hostd fixture used by cross-process E2E tests.

use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use async_trait::async_trait;
use piko_hostd::{
    OrchAgentRunRunner,
    domain::config::{HostSettings, McpServerConfig, SafetySettings},
    protocol::{HostServer, run_jsonl_server},
};
use piko_llmd::gateway::{
    ConversationItemKind, FinishReason, InferenceError, InferenceEvent, InferenceExecution,
    InferenceGateway, InferenceRequest,
};
use piko_protocol::{MessageContent, Usage};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader, ReadBuf},
    sync::mpsc,
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum ScriptMode {
    Steer,
    Queue,
    Immediate,
    Read,
    Edit,
    Write,
    Exec,
    Environment,
    Interaction,
    Todo,
    Cancel,
    MultiAgent,
    Compact,
    Mcp,
}

impl ScriptMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "steer" => Ok(Self::Steer),
            "queue" => Ok(Self::Queue),
            "immediate" => Ok(Self::Immediate),
            "read" => Ok(Self::Read),
            "edit" => Ok(Self::Edit),
            "write" => Ok(Self::Write),
            "exec" => Ok(Self::Exec),
            "environment" => Ok(Self::Environment),
            "interaction" => Ok(Self::Interaction),
            "todo" => Ok(Self::Todo),
            "cancel" => Ok(Self::Cancel),
            "multi-agent" => Ok(Self::MultiAgent),
            "compact" => Ok(Self::Compact),
            "mcp" => Ok(Self::Mcp),
            other => Err(format!("unsupported e2e mode: {other}")),
        }
    }
}

struct ScriptedGateway {
    mode: ScriptMode,
    step: AtomicUsize,
    release_path: PathBuf,
    log_path: PathBuf,
}

impl ScriptedGateway {
    fn new(mode: ScriptMode, release_path: PathBuf, log_path: PathBuf) -> Self {
        Self {
            mode,
            step: AtomicUsize::new(0),
            release_path,
            log_path,
        }
    }

    fn response_text(&self, step: usize) -> &'static str {
        match (self.mode, step) {
            (ScriptMode::Steer, 2) => "steer acknowledged",
            (ScriptMode::Queue, 2) => "follow-up acknowledged",
            (ScriptMode::Read, 2) => "read acknowledged",
            (ScriptMode::Edit, 2) => "edit acknowledged",
            (ScriptMode::Write, 2) => "write acknowledged",
            (ScriptMode::Exec, 2) => "exec acknowledged",
            (ScriptMode::Environment, 2) => "environment acknowledged",
            (ScriptMode::Interaction, 2) => "interaction acknowledged",
            (ScriptMode::Todo, 2) => "todo acknowledged",
            (ScriptMode::MultiAgent, 2) => "child acknowledged",
            (ScriptMode::MultiAgent, 3) => "parent acknowledged",
            (ScriptMode::Compact, 2) => "history summary",
            _ => "turn complete",
        }
    }

    fn tool_call(&self) -> Option<(&'static str, &'static str, &'static str)> {
        match self.mode {
            ScriptMode::Read => Some((
                "call-read",
                "read",
                r#"{"path":"e2e-input.txt","line_start":1,"line_end":10}"#,
            )),
            ScriptMode::Edit => Some((
                "call-edit",
                "edit",
                r#"{"path":"e2e-input.txt","edits":[{"oldText":"before","newText":"after"}]}"#,
            )),
            ScriptMode::Write => Some((
                "call-write",
                "write",
                r#"{"path":"e2e-output.txt","content":"written by piko e2e\n"}"#,
            )),
            ScriptMode::Exec => Some((
                "call-exec",
                "exec_command",
                r#"{"cmd":"sleep 30","yield_time_ms":0,"sandbox_permissions":"require_escalated","justification":"exercise process control"}"#,
            )),
            ScriptMode::Environment => Some(("call-environment", "environment", r#"{}"#)),
            ScriptMode::Interaction => Some((
                "call-interaction",
                "ask_user",
                r#"{"question":"Should the scripted turn continue?"}"#,
            )),
            ScriptMode::Todo => Some((
                "call-todo",
                "todo_write",
                r#"{"todos":[{"id":"e2e","content":"verify the E2E path","status":"in_progress"}]}"#,
            )),
            ScriptMode::MultiAgent => Some((
                "call-spawn",
                "spawn_agent",
                r#"{"agent_spec_id":"general","prompt":"inspect this subtask"}"#,
            )),
            _ => None,
        }
    }
}

#[async_trait]
impl InferenceGateway for ScriptedGateway {
    #[allow(clippy::disallowed_methods)]
    async fn start(
        &self,
        request: InferenceRequest,
        cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        let step = self.step.fetch_add(1, Ordering::SeqCst) + 1;
        let user_messages = request
            .conversation
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ConversationItemKind::User { content } => Some(content_text(content)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let context_messages = request
            .conversation
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ConversationItemKind::Context {
                    content, source, ..
                } => Some(json!({
                    "content": content_text(content),
                    "source": source,
                })),
                _ => None,
            })
            .collect::<Vec<_>>();
        append_record(
            &self.log_path,
            "gateway",
            json!({
                "step": step,
                "model": {
                    "provider": request.model.provider,
                    "id": request.model.model,
                },
                "user_messages": user_messages,
                "context_messages": context_messages,
            }),
        );

        if matches!(self.mode, ScriptMode::Cancel) && step == 1 {
            let (sender, receiver) = mpsc::channel(16);
            let release_path = self.release_path.clone();
            tokio::spawn(async move {
                let _ = sender.send(InferenceEvent::text("initial response")).await;
                loop {
                    if release_path.exists() {
                        break;
                    }
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            let _ = sender
                                .send(InferenceEvent::Completed(FinishReason::Cancelled))
                                .await;
                            return;
                        }
                        _ = tokio::time::sleep(Duration::from_millis(10)) => {}
                    }
                }
                let _ = sender.send(InferenceEvent::Usage(Usage::empty())).await;
                let _ = sender.send(InferenceEvent::completed("stop")).await;
            });
            return Ok(InferenceExecution {
                events: Box::pin(ReceiverStream::new(receiver)),
                handle: None,
            });
        }

        if step == 1
            && let Some((call_id, name, arguments)) = self.tool_call()
        {
            let events = vec![
                InferenceEvent::function_call(call_id, name, arguments),
                InferenceEvent::Usage(Usage::empty()),
                InferenceEvent::completed("tool_use"),
            ];
            return Ok(InferenceExecution {
                events: Box::pin(tokio_stream::iter(events)),
                handle: None,
            });
        }

        if matches!(self.mode, ScriptMode::Steer | ScriptMode::Queue) && step == 1 {
            let (sender, receiver) = mpsc::channel(16);
            let release_path = self.release_path.clone();
            tokio::spawn(async move {
                let _ = sender.send(InferenceEvent::text("initial response")).await;
                loop {
                    if release_path.exists() {
                        break;
                    }
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep(Duration::from_millis(10)) => {}
                    }
                }
                let _ = sender.send(InferenceEvent::Usage(Usage::empty())).await;
                let _ = sender.send(InferenceEvent::completed("stop")).await;
            });
            return Ok(InferenceExecution {
                events: Box::pin(ReceiverStream::new(receiver)),
                handle: None,
            });
        }

        let events = vec![
            InferenceEvent::text(self.response_text(step)),
            InferenceEvent::Usage(Usage::empty()),
            InferenceEvent::completed("stop"),
        ];
        Ok(InferenceExecution {
            events: Box::pin(tokio_stream::iter(events)),
            handle: None,
        })
    }
}

async fn build_runner(gateway: Arc<ScriptedGateway>, mode: ScriptMode) -> Arc<OrchAgentRunRunner> {
    let gateway: Arc<dyn InferenceGateway> = gateway;
    let mcp_configs = if matches!(mode, ScriptMode::Mcp) {
        vec![McpServerConfig {
            name: "e2e-mcp".into(),
            command: "echo".into(),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            timeout_ms: Some(1_000),
        }]
    } else {
        Vec::new()
    };
    let safety = matches!(mode, ScriptMode::Write | ScriptMode::Edit).then(|| SafetySettings {
        auto_approve_workspace_writes: Some(false),
    });
    if !mcp_configs.is_empty() || safety.is_some() {
        Arc::new(
            OrchAgentRunRunner::new_with_mcp(
                gateway,
                "scripted",
                "scripted-model",
                None,
                128_000,
                4_096,
                &mcp_configs,
                None,
                None,
                None,
                None,
                safety.as_ref(),
                None,
                None,
                None,
                piko_hostd::telemetry::handle(),
            )
            .await,
        )
    } else {
        Arc::new(OrchAgentRunRunner::new(gateway, "scripted", "scripted-model").await)
    }
}

fn content_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => text.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .map(piko_protocol::ContentBlock::text_projection)
            .collect::<Vec<_>>()
            .join(""),
    }
}

struct LoggingReader<R: AsyncRead> {
    inner: BufReader<R>,
    log_path: PathBuf,
    pending: Vec<u8>,
}

impl<R: AsyncRead> LoggingReader<R> {
    fn new(inner: R, log_path: PathBuf) -> Self {
        Self {
            inner: BufReader::new(inner),
            log_path,
            pending: Vec::new(),
        }
    }

    fn record_complete_lines(&mut self) {
        while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.pending.drain(..=index).collect();
            if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                append_record(&self.log_path, "command", value);
            }
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for LoggingReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buffer)
    }
}

impl<R: AsyncRead + Unpin> AsyncBufRead for LoggingReader<R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        Pin::new(&mut self.get_mut().inner).poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amount: usize) {
        let bytes = self.inner.buffer()[..amount].to_vec();
        self.inner.consume(amount);
        self.pending.extend(bytes);
        self.record_complete_lines();
    }
}

struct LoggingWriter<W> {
    inner: W,
    log_path: PathBuf,
    pending: Vec<u8>,
}

impl<W> LoggingWriter<W> {
    fn new(inner: W, log_path: PathBuf) -> Self {
        Self {
            inner,
            log_path,
            pending: Vec::new(),
        }
    }

    fn record_complete_lines(&mut self) {
        while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.pending.drain(..=index).collect();
            if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                append_record(&self.log_path, "event", value);
            }
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for LoggingWriter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::new(&mut self.inner).poll_write(cx, buffer) {
            Poll::Ready(Ok(written)) => {
                self.pending.extend_from_slice(&buffer[..written]);
                self.record_complete_lines();
                Poll::Ready(Ok(written))
            }
            other => other,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

fn append_record(path: &Path, kind: &str, value: Value) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open e2e trace");
    serde_json::to_writer(&mut file, &json!({ "kind": kind, "value": value }))
        .expect("serialize e2e trace");
    file.write_all(b"\n").expect("write e2e trace");
    file.flush().expect("flush e2e trace");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = ScriptMode::parse(&std::env::var("PIKO_TUI_E2E_MODE")?)?;
    let release_path = PathBuf::from(std::env::var("PIKO_TUI_E2E_RELEASE")?);
    let log_path = PathBuf::from(std::env::var("PIKO_TUI_PTY_LOG")?);
    let gateway_log_path = std::env::var_os("PIKO_TUI_E2E_GATEWAY_LOG")
        .map(PathBuf::from)
        .unwrap_or_else(|| log_path.clone());
    let session_root = PathBuf::from(std::env::var("PIKO_SESSION_DIR")?);

    let gateway = Arc::new(ScriptedGateway::new(mode, release_path, gateway_log_path));
    let runner = build_runner(gateway.clone(), mode).await;
    let server = HostServer::with_storage_runner_settings(
        piko_hostd::infra::storage::JsonlSessionRepository::new(session_root),
        runner,
        HostSettings::default(),
    );
    if matches!(mode, ScriptMode::Compact) {
        server.set_model_executor(gateway).await;
    }
    run_jsonl_server(
        LoggingReader::new(tokio::io::stdin(), log_path.clone()),
        LoggingWriter::new(tokio::io::stdout(), log_path),
        server,
    )
    .await
}
