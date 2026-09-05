use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use piko_hostd::{
    OrchAgentRunRunner,
    domain::config::{McpServerConfig, SafetySettings},
};
use piko_llmd::gateway::{
    ConversationItemKind, FinishReason, InferenceError, InferenceEvent, InferenceExecution,
    InferenceGateway, InferenceRequest,
};
use piko_protocol::{MessageContent, Usage};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
pub(crate) enum ScriptMode {
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
    HistorySoak,
}

impl ScriptMode {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
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
            "history-soak" => Ok(Self::HistorySoak),
            other => Err(format!("unsupported e2e mode: {other}")),
        }
    }

    pub(crate) fn uses_model_executor(self) -> bool {
        matches!(self, Self::Compact | Self::HistorySoak)
    }
}

pub(crate) struct ScriptedGateway {
    mode: ScriptMode,
    step: AtomicUsize,
    release_path: PathBuf,
    log_path: PathBuf,
}

impl ScriptedGateway {
    pub(crate) fn new(mode: ScriptMode, release_path: PathBuf, log_path: PathBuf) -> Self {
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
            (ScriptMode::Write, 2) | (ScriptMode::HistorySoak, 5) => "write acknowledged",
            (ScriptMode::Exec, 2) => "exec acknowledged",
            (ScriptMode::Environment, 2) => "environment acknowledged",
            (ScriptMode::Interaction, 2) => "interaction acknowledged",
            (ScriptMode::Todo, 2) => "todo acknowledged",
            (ScriptMode::MultiAgent, 2) | (ScriptMode::HistorySoak, 2) => "child acknowledged",
            (ScriptMode::MultiAgent, 3) | (ScriptMode::HistorySoak, 3) => "parent acknowledged",
            (ScriptMode::Compact, 2) => "history summary",
            (ScriptMode::HistorySoak, step) if step >= 6 => "history summary",
            _ => "turn complete",
        }
    }

    fn tool_call(&self, step: usize) -> Option<(&'static str, &'static str, &'static str)> {
        match (self.mode, step) {
            (ScriptMode::Read, 1) => Some((
                "call-read",
                "read",
                r#"{"path":"e2e-input.txt","line_start":1,"line_end":10}"#,
            )),
            (ScriptMode::Edit, 1) => Some((
                "call-edit",
                "edit",
                r#"{"path":"e2e-input.txt","edits":[{"oldText":"before","newText":"after"}]}"#,
            )),
            (ScriptMode::Write, 1) | (ScriptMode::HistorySoak, 4) => Some((
                "call-write",
                "write",
                r#"{"path":"e2e-output.txt","content":"written by piko e2e\n"}"#,
            )),
            (ScriptMode::Exec, 1) => Some((
                "call-exec",
                "exec_command",
                r#"{"cmd":"sleep 30","yield_time_ms":0,"sandbox_permissions":"require_escalated","justification":"exercise process control"}"#,
            )),
            (ScriptMode::Environment, 1) => Some(("call-environment", "environment", r#"{}"#)),
            (ScriptMode::Interaction, 1) => Some((
                "call-interaction",
                "ask_user",
                r#"{"question":"Should the scripted turn continue?"}"#,
            )),
            (ScriptMode::Todo, 1) => Some((
                "call-todo",
                "todo_write",
                r#"{"todos":[{"id":"e2e","content":"verify the E2E path","status":"in_progress"}]}"#,
            )),
            (ScriptMode::MultiAgent, 1) | (ScriptMode::HistorySoak, 1) => Some((
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

        if matches!(self.mode, ScriptMode::Cancel) && step == 1
            || matches!(self.mode, ScriptMode::HistorySoak)
                && user_messages
                    .iter()
                    .any(|text| text.contains("cancel this"))
        {
            return paused(self.release_path.clone(), cancel, true);
        }
        if let Some((call_id, name, arguments)) = self.tool_call(step) {
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
        if matches!(
            (self.mode, step),
            (ScriptMode::Steer | ScriptMode::Queue, 1)
        ) {
            return paused(self.release_path.clone(), cancel, false);
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

#[allow(clippy::disallowed_methods)]
fn paused(
    release_path: PathBuf,
    cancel: CancellationToken,
    complete_on_cancel: bool,
) -> Result<InferenceExecution, InferenceError> {
    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(async move {
        let _ = sender.send(InferenceEvent::text("initial response")).await;
        loop {
            if release_path.exists() {
                break;
            }
            tokio::select! {
                _ = cancel.cancelled() => {
                    if complete_on_cancel {
                        let _ = sender
                            .send(InferenceEvent::Completed(FinishReason::Cancelled))
                            .await;
                    }
                    return;
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }
        let _ = sender.send(InferenceEvent::Usage(Usage::empty())).await;
        let _ = sender.send(InferenceEvent::completed("stop")).await;
    });
    Ok(InferenceExecution {
        events: Box::pin(ReceiverStream::new(receiver)),
        handle: None,
    })
}

pub(crate) async fn build_runner(
    gateway: Arc<ScriptedGateway>,
    mode: ScriptMode,
) -> Arc<OrchAgentRunRunner> {
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
    let safety = matches!(
        mode,
        ScriptMode::Write | ScriptMode::Edit | ScriptMode::HistorySoak
    )
    .then(|| SafetySettings {
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

pub(crate) fn append_record(path: &Path, kind: &str, value: Value) {
    use std::{fs::OpenOptions, io::Write};
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
