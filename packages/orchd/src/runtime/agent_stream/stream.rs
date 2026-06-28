// ---- agent_task_stream — async-stream based root agent execution ----

use std::collections::HashMap;

use async_stream::stream;
use futures_core::Stream;
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

// ---- Per-run context ----

pub(crate) struct RunContext {
    pub steer_tx: mpsc::UnboundedSender<SteerMessage>,
    pub child_tx: mpsc::UnboundedSender<Event>,
    pub cancel: CancellationToken,
}

// ---- LLM chunk accumulator ----
//
// Collects LLM stream chunks (tool calls, usage, stop reason) while the
// main loop yields TextDelta/ThinkingDelta directly (typewriter effect).
// After the stream ends, `build_message()` assembles the final
// `Message::Assistant` from accumulated state.

struct LlmChunks {
    text: String,
    reasoning: String,
    tool_calls: HashMap<usize, (String, String, serde_json::Value)>,
    usage: Option<crate::domain::model::transcript::MessageUsage>,
    stop_reason: String,
}

impl LlmChunks {
    fn new() -> Self {
        Self { text: String::new(), reasoning: String::new(), tool_calls: HashMap::new(), usage: None, stop_reason: "stop".into() }
    }

    fn apply_non_delta(&mut self, event: GatewayEvent) {
        match event {
            GatewayEvent::ToolCallStart { index, id, name, args } => { self.tool_calls.insert(index, (id, name, args)); }
            GatewayEvent::Usage(usage) => { self.usage = Some(usage); }
            GatewayEvent::Done(reason) => { self.stop_reason = reason; }
            GatewayEvent::Error(e) => { tracing::error!("Stream error: {e}"); self.stop_reason = "error".into(); }
            _ => {}
        }
    }

    fn build_message(&mut self, model: &ModelSpec) -> Message {
        let mut blocks = Vec::new();
        if !self.reasoning.is_empty() { blocks.push(ContentBlock::Thinking { thinking: std::mem::take(&mut self.reasoning), thinking_signature: None }); }
        if !self.text.is_empty() { blocks.push(ContentBlock::Text { text: std::mem::take(&mut self.text) }); }
        let mut sorted: Vec<_> = self.tool_calls.keys().copied().collect();
        sorted.sort_unstable();
        for idx in sorted {
            if let Some((id, name, args)) = self.tool_calls.remove(&idx) {
                blocks.push(ContentBlock::ToolCall { id, name, arguments: args, partial_json: None });
            }
        }
        if blocks.is_empty() { blocks.push(ContentBlock::Text { text: String::new() }); }
        Message::Assistant {
            content: blocks, api: "openai-completions".into(), provider: model.provider.clone(), model: model.id.clone(),
            usage: self.usage.clone(), stop_reason: Some(self.stop_reason.clone()), error_message: None,
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
        }
    }
}

// ---- Pure helpers (no yield) ----

fn extract_tool_calls(msg: &Message) -> Vec<ToolCallItem> {
    let mut tcs = Vec::new();
    if let Message::Assistant { content, .. } = msg {
        for (i, block) in content.iter().enumerate() {
            if let ContentBlock::ToolCall { id, name, arguments, .. } = block {
                tcs.push(ToolCallItem { content_index: i as u32, tool_call_index: tcs.len() as u32, id: id.clone(), name: name.clone(), arguments: arguments.clone() });
            }
        }
    }
    tcs
}

fn summarize(msg: &Message) -> String {
    let text: String = match msg {
        Message::Assistant { content, .. } => content.iter().filter_map(|b| match b { ContentBlock::Text { text } => Some(text.as_str()), _ => None }).collect::<Vec<_>>().join(""),
        _ => String::new(),
    };
    if text.len() > 200 { format!("{}...", &text[..200]) } else { text }
}

fn assistant_message_event(
    msg: &Message,
    message_id: &str,
    task_id: &str,
    agent_id: &str,
    host_context: Option<&crate::domain::tasks::task::HostTaskContext>,
) -> Option<Event> {
    let hc = host_context?;
    let (text, tool_calls, model, provider, usage, timestamp) = match msg {
        Message::Assistant { content, model, provider, usage, timestamp, .. } => {
            let text: String = content.iter().filter_map(|b| match b { ContentBlock::Text { text } => Some(text.as_str()), _ => None }).collect::<Vec<_>>().join("");
            let tool_calls: Vec<piko_protocol::ToolCallRef> = content.iter().filter_map(|b| match b {
                ContentBlock::ToolCall { id, name, arguments, .. } => Some(piko_protocol::ToolCallRef { id: id.clone(), name: name.clone(), args: arguments.clone() }),
                _ => None,
            }).collect();
            (text, tool_calls, model.clone(), provider.clone(), usage.clone(), *timestamp)
        }
        _ => return None,
    };
    Some(Event::AssistantMessageCompleted {
        session_id: hc.session_id.clone(),
        message_id: message_id.into(),
        task_id: task_id.into(),
        agent_id: agent_id.into(),
        text,
        tool_calls,
        model,
        provider,
        usage: usage.as_ref().map(|u| piko_protocol::Usage {
            input: u.input, output: u.output,
            cache_read: u.cache_read, cache_write: u.cache_write,
            total_tokens: u.total_tokens,
            cost: piko_protocol::UsageCost {
                input: u.cost.input, output: u.cost.output,
                cache_read: u.cost.cache_read, cache_write: u.cost.cache_write,
                total: u.cost.total,
            },
        }),
        timestamp: timestamp.unwrap_or(now_ms()),
    })
}

// ---- Entry point ----

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
                session_id: hc.session_id.clone(), task_id: task_id.clone(),
                agent_id: agent_id.clone(), timestamp: now_ms(),
            };
        }

        let mut transcript: Vec<Message> = task.history.clone().unwrap_or_default();
        if !task.prompt.trim().is_empty() {
            transcript.push(Message::User { content: MessageContent::String(task.prompt.clone()), timestamp: None });
        }

        let model_settings = deps.model_config.as_ref().map(|c| c.settings.clone())
            .unwrap_or(ModelRunSettings { allow_tool_calls: true, ..Default::default() });
        let model_config = deps.model_config.clone();
        let mut step_count: u32 = 0;

        'agent: loop {
            // ── Cancel check ──
            if ctx.cancel.is_cancelled() {
                if let Some(ref hc) = host_context {
                    yield Event::TaskCancelled { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), timestamp: now_ms() };
                }
                break 'agent;
            }

            // ── Drain external inputs ──
            while let Ok(msg) = steer_rx.try_recv() {
                transcript.push(Message::User { content: MessageContent::String(msg.message.clone()), timestamp: None });
                if let Some(ref hc) = host_context {
                    yield Event::TaskSteered { session_id: hc.session_id.clone(), task_id: task_id.clone(), source_task_id: msg.source_task_id, source_agent_id: msg.source_agent_id, message: msg.message, timestamp: now_ms() };
                }
            }
            while let Ok(event) = child_rx.try_recv() {
                yield event;
            }

            step_count += 1;

            // ── Discover tools ──
            let (tools, routes) = (*deps.tool_registry).discover_tools(&ToolDiscoveryContext {
                agent_id: agent_id.clone(), task_id: Some(task_id.clone()),
                tool_set_ids: spec.tool_set_ids.clone(), active_tool_names: spec.active_tool_names.clone(),
            }).await;

            if ctx.cancel.is_cancelled() {
                if let Some(ref hc) = host_context {
                    yield Event::TaskCancelled { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), timestamp: now_ms() };
                }
                break 'agent;
            }

            // ── Model step ──
            let msg_id = runtime_assistant_message_id(&task_id, &format!("step_{step_count}"));
            yield Event::MessageStart { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), role: MessageRole::Assistant };

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
                    yield Event::MessageEnd { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), stop_reason: Some("error".into()) };
                    if let Some(ref hc) = host_context {
                        yield Event::TaskFailed { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), error: format!("Gateway error: {e}"), timestamp: now_ms() };
                    }
                    break 'agent;
                }
            };

            let mut chunks = LlmChunks::new();

            // ── Stream LLM chunks ──
            while let Some(gw_event) = llm.next().await {
                if ctx.cancel.is_cancelled() { chunks.stop_reason = "abort".into(); break; }

                // Yield text/thinking deltas immediately for TUI typewriter effect
                match gw_event {
                    GatewayEvent::ContentDelta(ref delta) => {
                        chunks.text.push_str(delta);
                        yield Event::TextDelta { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), delta: delta.clone() };
                    }
                    GatewayEvent::ReasoningDelta(ref delta) => {
                        chunks.reasoning.push_str(delta);
                        yield Event::ThinkingDelta { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), delta: delta.clone() };
                    }
                    other => chunks.apply_non_delta(other),
                }
            }

            yield Event::MessageEnd { task_id: task_id.clone(), agent_id: agent_id.clone(), message_id: msg_id.clone(), stop_reason: Some(chunks.stop_reason.clone()) };

            let assistant_message = chunks.build_message(&model);
            transcript.push(assistant_message.clone());

            // Emit complete assembled message to TUI
            if let Some(event) = assistant_message_event(&assistant_message, &msg_id, &task_id, &agent_id, host_context.as_ref()) {
                yield event;
            }

            if ctx.cancel.is_cancelled() {
                if let Some(ref hc) = host_context {
                    yield Event::TaskCancelled { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), timestamp: now_ms() };
                }
                break 'agent;
            }

            // ── Process outcome ──
            let tool_calls = extract_tool_calls(&assistant_message);

            if tool_calls.is_empty() || !model_settings.allow_tool_calls {
                let summary = summarize(&assistant_message);
                if let Some(ref hc) = host_context {
                    yield Event::TaskCompleted { session_id: hc.session_id.clone(), task_id: task_id.clone(), agent_id: agent_id.clone(), total_steps: step_count, summary, final_status: "completed".into(), timestamp: now_ms() };
                }
                break 'agent;
            }

            // ── Execute tools ──
            let tool_events = tool_executor::execute_tool_calls_with_deps(
                &deps, &task_id, &agent_id, host_context.clone(), &tool_calls, &routes,
                &model_settings, ctx.cancel.clone(), &msg_id, &mut transcript, step_count,
            ).await;
            for event in tool_events { yield event; }
        }
    }
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}
