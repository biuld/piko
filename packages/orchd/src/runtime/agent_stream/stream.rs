// ---- agent_task_stream — async-stream based root agent execution ----
//
// Uses async-stream's `stream!` macro to produce a Stream<Item = Event>
// directly without AgentEventBuffer or hand-written poll state machine.

use std::collections::HashMap;

use async_stream::stream;
use futures_core::Stream;
use std::pin::Pin;
use futures_util::StreamExt;
use llmd::gateway::{GatewayEvent, GatewayRequest};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry};
use crate::domain::agents::spec::AgentSpec;
use crate::domain::events::event::Event;
use crate::domain::model::step::{ModelRunSettings, ModelSpec};
use crate::domain::model::transcript::{ContentBlock, Message, MessageContent};
use crate::domain::tasks::steering::SteerMessage;
use crate::domain::tasks::task::AgentTask;
use crate::domain::tools::definition::ToolDef;
use crate::ports::tool_provider::ToolDiscoveryContext;
use piko_protocol::MessageRole;
use piko_protocol::model::ModelProviderConfig;

use super::messages::*;
use super::tool_executor::{self, ToolCallItem};

// ---- Per-run context with only sender halves (no Clone needed) ----

pub(crate) struct RunContext {
    pub steer_tx: mpsc::UnboundedSender<SteerMessage>,
    pub child_tx: mpsc::UnboundedSender<Event>,
    pub cancel: CancellationToken,
}

// ---- Main entry point ----

pub(crate) fn root_agent_stream(
    ctx: RunContext,
    mut steer_rx: mpsc::UnboundedReceiver<SteerMessage>,
    mut child_rx: mpsc::UnboundedReceiver<Event>,
    deps: AgentRunDeps,
    task: AgentTask,
    spec: AgentSpec,
) -> impl Stream<Item = Event> {
    let task_id = task.id.clone().unwrap_or_default();
    let agent_id = spec.id.clone();
    let host_context = task.host_context.clone();

    stream! {
        // ── TaskStarted ──
        if let Some(ref hc) = host_context {
            yield Event::TaskStarted {
                session_id: hc.session_id.clone(),
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                timestamp: now_ms(),
            };
        }

        let mut transcript: Vec<Message> = task.history.clone().unwrap_or_default();
        if !task.prompt.trim().is_empty() {
            transcript.push(Message::User {
                content: MessageContent::String(task.prompt.clone()),
                timestamp: None,
            });
        }

        let model_settings = deps.model_config.as_ref().map(|c| c.settings.clone())
            .unwrap_or(ModelRunSettings { allow_tool_calls: true, ..Default::default() });
        let model_config = deps.model_config.clone();
        let tool_set_ids = spec.tool_set_ids.clone();
        let active_tool_names = spec.active_tool_names.clone();
        let mut step_count: u32 = 0;

        // ── Main loop ──
        'agent: loop {
            if ctx.cancel.is_cancelled() {
                if let Some(ref hc) = host_context {
                    yield Event::TaskCancelled { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), timestamp: now_ms() };
                }
                break 'agent;
            }

            // Drain steer
            while let Ok(msg) = steer_rx.try_recv() {
                transcript.push(Message::User { content: MessageContent::String(msg.message.clone()), timestamp: None });
                if let Some(ref hc) = host_context {
                    yield Event::TaskSteered { session_id: hc.session_id.clone(), task_id: task_id.clone(), source_task_id: msg.source_task_id, source_agent_id: msg.source_agent_id, message: msg.message, timestamp: now_ms() };
                }
            }

            // Drain child events
            while let Ok(event) = child_rx.try_recv() {
                yield event;
            }

            step_count += 1;

            // Discover tools
            let (tools, routes): (Vec<ToolDef>, HashMap<String, CatalogRoute>) = {
                (*deps.tool_registry).discover_tools(&ToolDiscoveryContext {
                    agent_id: agent_id.clone(), task_id: Some(task_id.clone()),
                    tool_set_ids: tool_set_ids.clone(), active_tool_names: active_tool_names.clone(),
                }).await
            };

            if ctx.cancel.is_cancelled() {
                if let Some(ref hc) = host_context {
                    yield Event::TaskCancelled { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), timestamp: now_ms() };
                }
                break 'agent;
            }

            // ── Model step ──
            let step_id = format!("step_{step_count}");
            let assistant_message_id = runtime_assistant_message_id(&task_id, &step_id);

            yield Event::MessageStart { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: assistant_message_id.clone(), role: MessageRole::Assistant };

            let (model, _provider) = model_config.as_ref().map(|c| (c.model.clone(), c.provider.clone()))
                .unwrap_or_else(|| (ModelSpec { id: "default".into(), name: "Default".into(), provider: "openai".into() }, ModelProviderConfig::default()));

            let request = GatewayRequest {
                run_id: task_id.clone(), step_id: step_id.clone(), transcript: transcript.clone(),
                system_prompt: spec.system_prompt.clone(),
                model: model.id.clone(), provider: model.provider.clone(), tools: tools.clone(),
                thinking: model_config.as_ref().and_then(|c| c.resolve_thinking()),
            };

            let mut llm_stream = match deps.model_executor.chat_stream(request, Some(ctx.cancel.clone())).await {
                Ok(s) => s,
                Err(e) => {
                    yield Event::MessageEnd { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: assistant_message_id.clone(), stop_reason: Some("error".into()) };
                    if let Some(ref hc) = host_context {
                        yield Event::TaskFailed { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), error: format!("Gateway error: {e}"), timestamp: now_ms() };
                    }
                    break 'agent;
                }
            };

            let mut current_text = String::new();
            let mut current_reasoning = String::new();
            let mut tool_calls_map: HashMap<usize, (String, String, serde_json::Value)> = HashMap::new();
            let mut final_usage = None;
            let mut final_stop_reason = "stop".to_string();

            while let Some(gw_event) = llm_stream.next().await {
                if ctx.cancel.is_cancelled() { final_stop_reason = "abort".to_string(); break; }
                match gw_event {
                    GatewayEvent::ContentDelta(delta) => {
                        current_text.push_str(&delta);
                        yield Event::TextDelta { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: assistant_message_id.clone(), delta };
                    }
                    GatewayEvent::ReasoningDelta(delta) => {
                        current_reasoning.push_str(&delta);
                        yield Event::ThinkingDelta { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: assistant_message_id.clone(), delta };
                    }
                    GatewayEvent::ToolCallStart { index, id, name, args } => {
                        tool_calls_map.insert(index, (id, name, args));
                    }
                    GatewayEvent::Usage(usage) => { final_usage = Some(usage); }
                    GatewayEvent::Done(reason) => { final_stop_reason = reason; }
                    GatewayEvent::Error(e) => { tracing::error!("Stream error: {e}"); final_stop_reason = "error".to_string(); break; }
                }
            }

            yield Event::MessageEnd { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: assistant_message_id.clone(), stop_reason: Some(final_stop_reason.clone()) };

            let mut blocks = Vec::new();
            if !current_reasoning.is_empty() { blocks.push(ContentBlock::Thinking { thinking: current_reasoning, thinking_signature: None }); }
            if !current_text.is_empty() { blocks.push(ContentBlock::Text { text: current_text }); }
            let mut sorted: Vec<_> = tool_calls_map.keys().copied().collect();
            sorted.sort_unstable();
            for idx in sorted {
                if let Some((id, name, args)) = tool_calls_map.remove(&idx) {
                    blocks.push(ContentBlock::ToolCall { id, name, arguments: args, partial_json: None });
                }
            }
            if blocks.is_empty() { blocks.push(ContentBlock::Text { text: String::new() }); }

            let assistant_message = Message::Assistant {
                content: blocks, api: "openai-completions".into(), provider: model.provider.clone(), model: model.id.clone(),
                usage: final_usage, stop_reason: Some(final_stop_reason), error_message: None,
                timestamp: Some(chrono::Utc::now().timestamp_millis()),
            };
            transcript.push(assistant_message.clone());

            if ctx.cancel.is_cancelled() {
                if let Some(ref hc) = host_context {
                    yield Event::TaskCancelled { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), timestamp: now_ms() };
                }
                break 'agent;
            }

            // Extract tool calls
            let tool_calls: Vec<ToolCallItem> = {
                let mut tcs = Vec::new();
                if let Message::Assistant { content, .. } = &assistant_message {
                    for (i, block) in content.iter().enumerate() {
                        if let ContentBlock::ToolCall { id, name, arguments, .. } = block {
                            tcs.push(ToolCallItem { content_index: i as u32, tool_call_index: tcs.len() as u32, id: id.clone(), name: name.clone(), arguments: arguments.clone() });
                        }
                    }
                }
                tcs
            };

            if tool_calls.is_empty() || !model_settings.allow_tool_calls {
                let text = content_text(&assistant_message);
                let summary = if text.len() > 200 { format!("{}...", &text[..200]) } else { text };
                if let Some(ref hc) = host_context {
                    yield Event::TaskCompleted { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), total_steps: step_count, summary, final_status: "completed".into(), timestamp: now_ms() };
                }
                break 'agent;
            }

            // Execute tools
            let tool_events = tool_executor::execute_tool_calls_with_deps(
                &deps, &task_id, &agent_id, host_context.clone(), &tool_calls, &routes, &model_settings,
                ctx.cancel.clone(), &assistant_message_id, &mut transcript, step_count,
            ).await;

            for event in tool_events { yield event; }
        }
    }
}
// ---- Helpers ----

fn content_text(msg: &Message) -> String {
    match msg {
        Message::Assistant { content, .. } => content.iter()
            .filter_map(|b| match b { ContentBlock::Text { text } => Some(text.as_str()), _ => None })
            .collect::<Vec<_>>().join(""),
        _ => String::new(),
    }
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}
