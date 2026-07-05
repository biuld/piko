// ---- AgentRun: tool executor — parallel / sequential execution ----

use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::events::event::Event;
use crate::domain::model::step::ModelRunSettings;
use crate::domain::model::transcript::{ContentBlock, Message};
use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::ToolExecutionMode;
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::ToolExecutionContext;
use piko_protocol::DisplayEvent;

use super::stream::AgentRunDeps;

/// Generate a stable runtime tool entity ID.
pub(crate) fn runtime_tool_entity_id(parent_message_id: &str, tool_call_index: u32) -> String {
    format!("{}:tool:{}", parent_message_id, tool_call_index)
}

// ---- Tool call descriptor ----

#[derive(Debug, Clone)]
pub struct ToolCallItem {
    pub content_index: u32,
    pub tool_call_index: u32,
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub(crate) struct AggregatedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Default)]
pub(crate) struct ToolCallAggregator {
    current: Option<(String, String, String)>,
    completed: Vec<AggregatedToolCall>,
}

impl ToolCallAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_chunk(&mut self, id: String, name: String, args_delta: String) {
        if !name.is_empty() {
            if let Some(prev) = self.current.take() {
                self.completed.push(Self::finalize(prev));
            }
            self.current = Some((id, name, args_delta));
        } else if let Some((_, _, args)) = self.current.as_mut() {
            args.push_str(&args_delta);
        }
    }

    pub fn flush(&mut self) -> Vec<AggregatedToolCall> {
        if let Some(prev) = self.current.take() {
            self.completed.push(Self::finalize(prev));
        }
        std::mem::take(&mut self.completed)
    }

    fn finalize((id, name, args): (String, String, String)) -> AggregatedToolCall {
        match serde_json::from_str::<serde_json::Value>(&args) {
            Ok(arguments) => AggregatedToolCall {
                id,
                name,
                arguments,
            },
            Err(_) => AggregatedToolCall {
                id,
                name,
                arguments: serde_json::Value::String(args),
            },
        }
    }
}

// ---- Public API ----

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_tool_calls_with_deps(
    deps: &AgentRunDeps,
    task_id: &str,
    agent_id: &str,
    host_context: Option<crate::domain::tasks::task::HostTaskContext>,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    model_settings: &ModelRunSettings,
    cancel: CancellationToken,
    parent_message_id: &str,
    transcript: &mut Vec<Message>,
    turn_index: u32,
) -> Vec<Event> {
    let mode = resolve_execution_mode(tool_calls, routes, model_settings);
    if mode == ExecutionMode::Parallel {
        execute_parallel_direct(
            deps,
            task_id,
            agent_id,
            host_context,
            tool_calls,
            routes,
            cancel,
            parent_message_id,
            transcript,
            turn_index,
        )
        .await
    } else {
        execute_sequential_direct(
            deps,
            task_id,
            agent_id,
            host_context,
            tool_calls,
            routes,
            cancel,
            parent_message_id,
            transcript,
            turn_index,
        )
        .await
    }
}

// ---- Execution mode resolution ----

#[derive(PartialEq)]
enum ExecutionMode {
    Parallel,
    Sequential,
}

fn resolve_execution_mode(
    calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    settings: &ModelRunSettings,
) -> ExecutionMode {
    if settings.parallel_tools == Some(false) {
        return ExecutionMode::Sequential;
    }
    for tc in calls {
        if let Some(route) = routes.get(&tc.name)
            && matches!(
                &route.tool_def.execution_mode,
                Some(ToolExecutionMode::Sequential)
            )
        {
            return ExecutionMode::Sequential;
        }
    }
    ExecutionMode::Parallel
}

// ---- Parallel execution ----

async fn execute_parallel_direct(
    deps: &AgentRunDeps,
    task_id: &str,
    agent_id: &str,
    host_context: Option<crate::domain::tasks::task::HostTaskContext>,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    parent_message_id: &str,
    transcript: &mut Vec<Message>,
    turn_index: u32,
) -> Vec<Event> {
    use crate::adapters::tools::registry::ToolRegistry;
    let mut futures = Vec::new();
    for tc in tool_calls {
        let route = routes.get(&tc.name).cloned();
        let (deps, agent_id, host_context, task_id, tc, pm_id, cancel) = (
            deps.clone(),
            agent_id.to_string(),
            host_context.clone(),
            task_id.to_string(),
            tc.clone(),
            parent_message_id.to_string(),
            cancel.clone(),
        );
        futures.push(async move {
            if cancel.is_cancelled() {
                return (
                    tc.clone(),
                    ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "cancelled".into(),
                            message: "Task cancelled".into(),
                            retryable: Some(false),
                        }),
                    },
                    vec![],
                );
            }
            if let Some(r) = route {
                let call = ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    partial_json: None,
                };
                let ctx = ToolExecutionContext {
                    agent_id,
                    task_id,
                    tool_set_ids: vec![],
                    turn_index: Some(turn_index),
                    event_seq: Some(0),
                    next_event_seq: None,
                    parent_message_id: Some(pm_id.clone()),
                    content_index: Some(tc.content_index),
                    tool_call_index: Some(tc.tool_call_index),
                    tool_entity_id: Some(runtime_tool_entity_id(&pm_id, tc.tool_call_index)),
                    host_context,
                };
                let rec = (*deps.tool_registry)
                    .execute_tool(&call, &ctx, &r, Some(cancel.clone()))
                    .await;
                (tc.clone(), rec.result, rec.events)
            } else {
                (
                    tc.clone(),
                    ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "not_found".into(),
                            message: format!("No route for tool \"{}\"", tc.name),
                            retryable: Some(false),
                        }),
                    },
                    vec![],
                )
            }
        });
    }
    let mut results: Vec<(ToolCallItem, ToolExecResult, Vec<Event>)> =
        futures_util::future::join_all(futures).await;
    results.sort_by_key(|(tc, _, _)| tc.tool_call_index);
    let mut events = Vec::new();
    for (tc, r, ev) in results {
        events.extend(ev);
        let msg = append_tool(transcript, &tc, &r);
        if let Some(ref hc) = host_context {
            events.push(Event::Display(DisplayEvent::ToolResultCommitted {
                session_id: hc.session_id.clone(),
                message_id: format!("{task_id}:tool_result:{}", tc.id),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                message: msg,
            }));
        }
    }
    events
}

// ---- Sequential execution ----

async fn execute_sequential_direct(
    deps: &AgentRunDeps,
    task_id: &str,
    agent_id: &str,
    host_context: Option<crate::domain::tasks::task::HostTaskContext>,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    parent_message_id: &str,
    transcript: &mut Vec<Message>,
    turn_index: u32,
) -> Vec<Event> {
    use crate::adapters::tools::registry::ToolRegistry;
    let mut events = Vec::new();
    for tc in tool_calls {
        if cancel.is_cancelled() {
            let msg = append_tool_err(transcript, tc, "Task cancelled");
            if let Some(ref hc) = host_context {
                events.push(Event::Display(DisplayEvent::ToolResultCommitted {
                    session_id: hc.session_id.clone(),
                    message_id: format!("{task_id}:tool_result:{}", tc.id),
                    task_id: task_id.to_string(),
                    agent_id: agent_id.to_string(),
                    message: msg,
                }));
            }
            continue;
        }
        let r = match routes.get(&tc.name) {
            Some(r) => r,
            None => {
                let msg = append_tool_err(
                    transcript,
                    tc,
                    &format!("No route for tool \"{}\"", tc.name),
                );
                if let Some(ref hc) = host_context {
                    events.push(Event::Display(DisplayEvent::ToolResultCommitted {
                        session_id: hc.session_id.clone(),
                        message_id: format!("{task_id}:tool_result:{}", tc.id),
                        task_id: task_id.to_string(),
                        agent_id: agent_id.to_string(),
                        message: msg,
                    }));
                }
                continue;
            }
        };
        let call = ToolCall {
            id: tc.id.clone(),
            name: tc.name.clone(),
            arguments: tc.arguments.clone(),
            partial_json: None,
        };
        let ctx = ToolExecutionContext {
            agent_id: agent_id.to_string(),
            task_id: task_id.to_string(),
            tool_set_ids: vec![],
            turn_index: Some(turn_index),
            event_seq: Some(0),
            next_event_seq: None,
            parent_message_id: Some(parent_message_id.to_string()),
            content_index: Some(tc.content_index),
            tool_call_index: Some(tc.tool_call_index),
            tool_entity_id: Some(runtime_tool_entity_id(
                parent_message_id,
                tc.tool_call_index,
            )),
            host_context: host_context.clone(),
        };
        let rec = (*deps.tool_registry)
            .execute_tool(&call, &ctx, r, Some(cancel.clone()))
            .await;
        events.extend(rec.events);
        let msg = append_tool(transcript, tc, &rec.result);
        if let Some(ref hc) = host_context {
            events.push(Event::Display(DisplayEvent::ToolResultCommitted {
                session_id: hc.session_id.clone(),
                message_id: format!("{task_id}:tool_result:{}", tc.id),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                message: msg,
            }));
        }
    }
    events
}

// ---- Append helpers ----

fn append_tool(
    transcript: &mut Vec<Message>,
    tc: &ToolCallItem,
    result: &ToolExecResult,
) -> Message {
    let msg = if result.ok {
        let text = match &result.value {
            Some(v) if v.is_string() => v.as_str().unwrap_or("").to_string(),
            Some(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
            None => String::new(),
        };
        Message::ToolResult {
            tool_call_id: tc.id.clone(),
            tool_name: Some(tc.name.clone()),
            content: vec![ContentBlock::Text { text }],
            details: result.value.clone(),
            is_error: Some(false),
            timestamp: None,
        }
    } else {
        let msg = result
            .error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_else(|| "Unknown error".into());
        Message::ToolResult {
            tool_call_id: tc.id.clone(),
            tool_name: Some(tc.name.clone()),
            content: vec![ContentBlock::Text { text: msg }],
            details: result
                .error
                .as_ref()
                .map(|e| serde_json::to_value(e).unwrap_or_default()),
            is_error: Some(true),
            timestamp: None,
        }
    };
    transcript.push(msg.clone());
    msg
}

fn append_tool_err(transcript: &mut Vec<Message>, tc: &ToolCallItem, error: &str) -> Message {
    let msg = Message::ToolResult {
        tool_call_id: tc.id.clone(),
        tool_name: Some(tc.name.clone()),
        content: vec![ContentBlock::Text {
            text: format!("Tool error: {error}"),
        }],
        details: Some(serde_json::json!({"error": error})),
        is_error: Some(true),
        timestamp: None,
    };
    transcript.push(msg.clone());
    msg
}
