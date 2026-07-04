// ---- agent_task_stream — async-stream based root agent execution ----

use std::sync::Arc;

use async_stream::stream;
use futures_core::Stream;
use futures_util::StreamExt;
use llmd::gateway::{GatewayEvent, GatewayRequest};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::ToolRegistry;
use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::events::event::Event;
use crate::domain::model::step::{ModelConfig, ModelRunSettings, ModelSpec};
use crate::domain::model::transcript::{
    AssistantContentBlock, ContentBlock, Message, MessageContent,
};
use crate::domain::tasks::steering::SteerMessage;
use crate::domain::tasks::task::{AgentTask, HostTaskContext};
use crate::ports::agent_spawner::AgentSpawner;
use crate::ports::model_gateway::LlmGateway;
use crate::ports::tool_provider::ToolDiscoveryContext;
use piko_protocol::MessageRole;
use piko_protocol::{MessageEvent, TaskEvent, ToolEvent};

use super::chunks::LlmChunks;
use super::tool_executor;

// ---- Agent run dependencies ----

/// Dependencies injected into an agent run.
#[derive(Clone)]
pub(crate) struct AgentRunDeps {
    pub model_executor: Arc<dyn LlmGateway>,
    pub model_config: Option<ModelConfig>,
    pub tool_registry: Arc<ToolRegistryImpl>,
}

// ---- Per-run context ----

pub(crate) struct RunContext {
    #[allow(dead_code)] // held to keep channel alive
    pub steer_tx: mpsc::UnboundedSender<SteerMessage>,
    pub cancel: CancellationToken,
}

// ---- Pure helpers (no yield) ----

fn summarize(msg: &Message) -> String {
    let text: String = match msg {
        Message::Assistant { content, .. } => content
            .iter()
            .filter_map(|b| match b {
                AssistantContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    };
    if text.len() > 200 {
        format!("{}...", &text[..200])
    } else {
        text
    }
}

fn assistant_message_event(
    msg: &Message,
    message_id: &str,
    task_id: &str,
    agent_id: &str,
    host_context: Option<&crate::domain::tasks::task::HostTaskContext>,
) -> Option<Event> {
    let hc = host_context?;
    if !matches!(msg, Message::Assistant { .. }) {
        return None;
    }
    Some(Event::Message(MessageEvent::AssistantCompleted {
        session_id: hc.session_id.clone(),
        message_id: message_id.into(),
        task_id: task_id.into(),
        agent_id: agent_id.into(),
        message: msg.clone(),
    }))
}

fn tool_call_message_event(
    msg: &Message,
    message_id: &str,
    parent_message_id: &str,
    task_id: &str,
    agent_id: &str,
    host_context: Option<&crate::domain::tasks::task::HostTaskContext>,
) -> Option<Event> {
    let hc = host_context?;
    if !matches!(msg, Message::ToolCall { .. }) {
        return None;
    }
    Some(Event::Message(MessageEvent::ToolCallCommitted {
        session_id: hc.session_id.clone(),
        message_id: message_id.into(),
        task_id: task_id.into(),
        agent_id: agent_id.into(),
        parent_message_id: parent_message_id.into(),
        message: msg.clone(),
    }))
}

// ---- Entry point: unified agent loop ----

pub(crate) fn agent_loop(
    ctx: RunContext,
    steer_rx: mpsc::UnboundedReceiver<SteerMessage>,
    deps: AgentRunDeps,
    task: AgentTask,
    spec: AgentSpec,
    spawner: Arc<dyn AgentSpawner>,
) -> impl Stream<Item = Event> {
    let task_id = task.id.clone().unwrap_or_default();
    let agent_id = spec.id.clone();
    let host_context = task.host_context.clone();
    let source_agent_id = match &task.source {
        piko_protocol::agents::TaskSource::Agent { agent_id, .. } => Some(agent_id.clone()),
        _ => None,
    };

    stream! {
        // ── TaskCreated ──
        if let Some(ref hc) = host_context {
            yield Event::Task(TaskEvent::Created {
                session_id: hc.session_id.clone(),
                turn_id: hc.turn_id.clone(),
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                parent_task_id: task.parent_task_id.clone(),
                source_agent_id: source_agent_id.clone(),
                prompt: task.prompt.clone(),
                timestamp: now_ms(),
            });
        }

        // ── TaskStarted ──
        if let Some(ref hc) = host_context {
            yield Event::Task(TaskEvent::Started {
                session_id: hc.session_id.clone(), task_id: task_id.clone(),
                agent_id: agent_id.clone(), timestamp: now_ms(),
            });
        }

        let mut transcript: Vec<Message> = task.history.clone().unwrap_or_default();
        if !task.prompt.trim().is_empty() {
            transcript.push(Message::User { content: MessageContent::String(task.prompt.clone()), timestamp: None });
        }

        let model_settings = deps.model_config.as_ref().map(|c| c.settings.clone())
            .unwrap_or(ModelRunSettings { allow_tool_calls: true, ..Default::default() });
        let model_config = deps.model_config.clone();
        let mut step_count: u32 = 0;
        let mut steer_rx = steer_rx;

        'agent: loop {
            // ── Cancel check ──
            if ctx.cancel.is_cancelled() {
                if let Some(ref hc) = host_context {
                    yield Event::Task(TaskEvent::Cancelled { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), timestamp: now_ms() });
                }
                break 'agent;
            }

            // ── Drain steering messages ──
            while let Ok(msg) = steer_rx.try_recv() {
                transcript.push(Message::User { content: MessageContent::String(msg.message.clone()), timestamp: None });
                if let Some(ref hc) = host_context {
                    yield Event::Task(TaskEvent::Steered { session_id: hc.session_id.clone(), task_id: task_id.clone(), source_task_id: msg.source_task_id, source_agent_id: msg.source_agent_id, message: msg.message, timestamp: now_ms() });
                }
            }

            step_count += 1;

            // ── Discover tools ──
            let (tools, routes) = (*deps.tool_registry).discover_tools(&ToolDiscoveryContext {
                agent_id: agent_id.clone(), task_id: Some(task_id.clone()),
                tool_set_ids: spec.tool_set_ids.clone(), active_tool_names: spec.active_tool_names.clone(),
            }).await;

            if ctx.cancel.is_cancelled() {
                if let Some(ref hc) = host_context {
                    yield Event::Task(TaskEvent::Cancelled { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), timestamp: now_ms() });
                }
                break 'agent;
            }

            // ── Model step ──
            let msg_id = runtime_assistant_message_id(&task_id, &format!("step_{step_count}"));
            yield Event::Message(MessageEvent::Start { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), role: MessageRole::Assistant });

            let model = model_config.as_ref().map(|c| c.model.clone())
                .unwrap_or(ModelSpec { id: "default".into(), name: "Default".into(), provider: "openai".into() });

            let request = GatewayRequest {
                run_id: task_id.clone(), step_id: format!("step_{step_count}"), transcript: transcript.clone(),
                system_prompt: spec.system_prompt.clone(), model: model.id.clone(), provider: model.provider.clone(),
                tools: tools.clone(), thinking: model_config.as_ref().and_then(|c| c.resolve_thinking()),
            };

            let mut llm = match deps.model_executor.chat_stream(request, Some(ctx.cancel.clone())).await {
                Ok(s) => s,
                Err(e) => {
                    yield Event::Message(MessageEvent::End { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), stop_reason: Some("error".into()) });
                    if let Some(ref hc) = host_context {
                        yield Event::Task(TaskEvent::Failed { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), error: format!("Gateway error: {e}"), timestamp: now_ms() });
                    }
                    break 'agent;
                }
            };

            let mut chunks = LlmChunks::new();

            // ── Stream LLM chunks ──
            while let Some(gw_event) = llm.next().await {
                if ctx.cancel.is_cancelled() { chunks.stop_reason = "abort".into(); break; }
                match gw_event {
                    GatewayEvent::ContentDelta(ref delta) => {
                        chunks.text.push_str(delta);
                        yield Event::Message(MessageEvent::TextDelta { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), content_index: chunks.text.len() as u32, delta: delta.clone() });
                    }
                    GatewayEvent::ReasoningDelta(ref delta) => {
                        chunks.reasoning.push_str(delta);
                        yield Event::Message(MessageEvent::ThinkingDelta { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), content_index: chunks.reasoning.len() as u32, delta: delta.clone() });
                    }
                    other => chunks.apply_non_delta(other),
                }
            }

            yield Event::Message(MessageEvent::End { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), stop_reason: Some(chunks.stop_reason.clone()) });

            let assistant_message = chunks.build_message(&model);
            transcript.push(assistant_message.clone());

            if let Some(event) = assistant_message_event(&assistant_message, &msg_id, &task_id, &agent_id, host_context.as_ref()) {
                yield event;
            }

            let tool_calls = chunks.take_tool_calls();
            for tc in &tool_calls {
                let tool_call_message_id = runtime_tool_call_message_id(&msg_id, tc.tool_call_index);
                let tool_call_message = Message::ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    model: Some(model.id.clone()),
                    provider: Some(model.provider.clone()),
                    timestamp: Some(now_ms()),
                };
                transcript.push(tool_call_message.clone());
                if let Some(event) = tool_call_message_event(
                    &tool_call_message,
                    &tool_call_message_id,
                    &msg_id,
                    &task_id,
                    &agent_id,
                    host_context.as_ref(),
                ) {
                    yield event;
                }
            }

            if ctx.cancel.is_cancelled() {
                if let Some(ref hc) = host_context {
                    yield Event::Task(TaskEvent::Cancelled { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), timestamp: now_ms() });
                }
                break 'agent;
            }

            // ── Process outcome ──
            if tool_calls.is_empty() || !model_settings.allow_tool_calls {
                let summary = summarize(&assistant_message);
                if let Some(ref hc) = host_context {
                    yield Event::Task(TaskEvent::Completed { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), total_steps: step_count, summary, final_status: "completed".into(), timestamp: now_ms() });
                }
                break 'agent;
            }

            // ── Execute tools: split spawn tools (→ spawner) vs regular tools ──
            let (spawn_tools, regular_tools): (Vec<_>, Vec<_>) = tool_calls
                .into_iter()
                .partition(|tc| is_spawn_tool(&tc.name));

            // Regular tools: execute via tool registry
            if !regular_tools.is_empty() {
                let tool_events = tool_executor::execute_tool_calls_with_deps(
                    &deps, &task_id, &agent_id, host_context.clone(), &regular_tools, &routes,
                    &model_settings, ctx.cancel.clone(), &msg_id, &mut transcript, step_count,
                ).await;
                for event in tool_events { yield event; }
            }

            // Spawn tools: execute via AgentSpawner
            for tc in &spawn_tools {
                if host_context.is_some() {
                    yield Event::Tool(ToolEvent::Start {
                        task_id: task_id.clone(), agent_id: agent_id.clone(),
                        tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                        args: tc.arguments.clone(), parent_message_id: Some(msg_id.clone()),
                    });
                }

                let result = execute_spawn_tool(&spawner, &task_id, &host_context, &tc.name, &tc.arguments).await;
                let (tool_result, is_error) = match result {
                    Ok(value) => (value, false),
                    Err(err) => (serde_json::Value::String(err), true),
                };

                transcript.push(Message::ToolResult {
                    tool_call_id: tc.id.clone(),
                    tool_name: Some(tc.name.clone()),
                    content: vec![ContentBlock::Text { text: serde_json::to_string_pretty(&tool_result).unwrap_or_default() }],
                    details: Some(tool_result.clone()),
                    is_error: Some(is_error),
                    timestamp: None,
                });

                yield Event::Tool(ToolEvent::End {
                    task_id: task_id.clone(), agent_id: agent_id.clone(),
                    tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                    result: tool_result, is_error,
                });
            }
        }
    }
}

/// Execute a spawn-related tool call through the AgentSpawner trait.
async fn execute_spawn_tool(
    spawner: &Arc<dyn AgentSpawner>,
    parent_task_id: &str,
    host_context: &Option<crate::domain::tasks::task::HostTaskContext>,
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let hc = host_context.clone().unwrap_or_else(|| HostTaskContext {
        session_id: String::new(),
        turn_id: String::new(),
    });
    let agent_id = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
    let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

    match tool_name {
        "spawn" => match spawner
            .spawn(agent_id, prompt, Some(parent_task_id.to_string()), hc)
            .await
        {
            Some(report) => {
                let mut json = serde_json::json!({
                    "status": report.status,
                    "text": report.text,
                    "total_steps": report.total_steps,
                });
                if let Some(ref tid) = report.task_id {
                    json["task_id"] = serde_json::Value::String(tid.clone());
                }
                Ok(json)
            }
            None => Err(format!("spawn on agent '{}' returned no result", agent_id)),
        },
        "spawn_detached" => {
            let task_id = spawner
                .spawn_detached(agent_id, prompt, Some(parent_task_id.to_string()), hc)
                .await;
            Ok(serde_json::json!({"task_id": task_id, "status": "detached"}))
        }
        "poll_task" => {
            let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            let timeout_ms = args.get("timeout_ms").and_then(|v| v.as_u64());
            match spawner.poll_task(task_id, timeout_ms).await {
                Some(report) => Ok(serde_json::json!({
                    "status": report.status,
                    "text": report.text,
                    "total_steps": report.total_steps,
                    "task_id": task_id,
                })),
                None => Err(format!("task {} not found or not completed", task_id)),
            }
        }
        "steer_task" => {
            let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
            let ok = spawner.steer_task(task_id, message).await;
            Ok(serde_json::json!({"ok": ok}))
        }
        _ => Err(format!("unknown spawn tool: {}", tool_name)),
    }
}

fn is_spawn_tool(name: &str) -> bool {
    matches!(
        name,
        "spawn" | "spawn_detached" | "poll_task" | "steer_task"
    )
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Produce a stable runtime assistant message ID.
pub fn runtime_assistant_message_id(run_id: &str, step_id: &str) -> String {
    format!("{run_id}:{step_id}:assistant")
}

pub fn runtime_tool_call_message_id(parent_message_id: &str, tool_call_index: u32) -> String {
    format!("{parent_message_id}:tool_call:{tool_call_index}")
}
