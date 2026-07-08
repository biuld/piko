// ---- AgentRun: tool executor — parallel / sequential execution ----

use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use super::dispatch::ToolExecutionConsumer;
use super::stream::AgentRunDeps;
use super::stream::TranscriptManager;
use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::events::event::Event;
use crate::domain::model::step::ModelRunSettings;
use crate::domain::model::transcript::{ContentBlock, Message};
use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::ToolExecutionMode;
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::agent_spawner::AgentSpawner;
use crate::ports::tool_provider::ToolExecutionContext;
use crate::runtime::tool_calls::ToolCallItem;

/// Generate a stable runtime tool entity ID.
pub(crate) fn runtime_tool_entity_id(parent_message_id: &str, tool_call_index: u32) -> String {
    format!("{}:tool:{}", parent_message_id, tool_call_index)
}

pub(crate) struct ToolExecutionResult {
    pub events: Vec<Event>,
    pub completed_calls: usize,
    pub failed_calls: usize,
}

// ---- Public API ----

pub(crate) async fn execute_tool_calls_with_deps(
    deps: &AgentRunDeps,
    spawner: &std::sync::Arc<dyn AgentSpawner>,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    model_settings: &ModelRunSettings,
    cancel: CancellationToken,
    transcript: &mut TranscriptManager,
    turn_index: u32,
    tool_consumer: &ToolExecutionConsumer,
) -> Result<ToolExecutionResult, String> {
    let mode = resolve_execution_mode(tool_calls, routes, model_settings);
    if mode == ExecutionMode::Parallel {
        execute_parallel_direct(
            deps,
            spawner,
            tool_calls,
            routes,
            cancel.clone(),
            transcript,
            turn_index,
            tool_consumer,
        )
        .await
    } else {
        execute_sequential_direct(
            deps,
            spawner,
            tool_calls,
            routes,
            cancel.clone(),
            transcript,
            turn_index,
            tool_consumer,
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
        if is_spawn_tool(&tc.name) {
            return ExecutionMode::Sequential;
        }
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
    _spawner: &std::sync::Arc<dyn AgentSpawner>,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    transcript: &mut TranscriptManager,
    turn_index: u32,
    tool_consumer: &ToolExecutionConsumer,
) -> Result<ToolExecutionResult, String> {
    use crate::adapters::tools::registry::ToolRegistry;
    let mut futures = Vec::new();
    for tc in tool_calls {
        let route = routes.get(&tc.name).cloned();
        let tool_consumer = tool_consumer.clone();
        let (deps, tc, cancel) = (deps.clone(), tc.clone(), cancel.clone());
        futures.push(async move {
            let mut events = Vec::new();

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
                    events,
                );
            }

            // Notify TUI: tool started
            if let Some(ev) = tool_consumer
                .tool_started(tc.id.clone(), tc.name.clone(), tc.arguments.clone())
                .await
            {
                events.push(ev);
            }

            if let Some(r) = route {
                let call = ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    partial_json: None,
                };
                let ctx = ToolExecutionContext {
                    agent_id: tool_consumer.agent_id().to_string(),
                    task_id: tool_consumer.task_id().to_string(),
                    tool_set_ids: vec![],
                    turn_index: Some(turn_index),
                    event_seq: Some(0),
                    next_event_seq: None,
                    parent_message_id: Some(tool_consumer.parent_message_id().to_string()),
                    content_index: Some(tc.content_index),
                    tool_call_index: Some(tc.tool_call_index),
                    tool_entity_id: Some(runtime_tool_entity_id(
                        tool_consumer.parent_message_id(),
                        tc.tool_call_index,
                    )),
                    host_context: tool_consumer.host_context().cloned(),
                    senders: tool_consumer.senders().clone(),
                };
                let rec = (*deps.tool_registry)
                    .execute_tool(&call, &ctx, &r, Some(cancel.clone()))
                    .await;

                // Notify TUI: tool ended
                if let Some(ev) = tool_consumer
                    .tool_ended(
                        tc.id.clone(),
                        tc.name.clone(),
                        rec.result.value.clone().unwrap_or(serde_json::Value::Null),
                        !rec.result.ok,
                    )
                    .await
                {
                    events.push(ev);
                }

                (tc.clone(), rec.result, events)
            } else {
                if let Some(ev) = tool_consumer
                    .tool_ended(
                        tc.id.clone(),
                        tc.name.clone(),
                        serde_json::Value::Null,
                        true,
                    )
                    .await
                {
                    events.push(ev);
                }
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
                    events,
                )
            }
        });
    }
    let mut results = futures_util::future::join_all(futures).await;
    results.sort_by_key(|(tc, _, _)| tc.tool_call_index);
    let mut output_events = Vec::new();
    let mut failed_calls = 0;
    for (tc, r, mut evs) in results {
        output_events.append(&mut evs);
        if !r.ok {
            failed_calls += 1;
        }
        let message = append_tool(transcript, &tc, &r);
        if let Some(ev) = tool_consumer
            .tool_result_committed(tc.tool_call_index, message)
            .await
        {
            output_events.push(ev);
        }
    }
    Ok(ToolExecutionResult {
        events: output_events,
        completed_calls: tool_calls.len(),
        failed_calls,
    })
}

// ---- Sequential execution ----

async fn execute_sequential_direct(
    deps: &AgentRunDeps,
    spawner: &std::sync::Arc<dyn AgentSpawner>,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    transcript: &mut TranscriptManager,
    turn_index: u32,
    tool_consumer: &ToolExecutionConsumer,
) -> Result<ToolExecutionResult, String> {
    use crate::adapters::tools::registry::ToolRegistry;
    let mut output_events = Vec::new();
    let mut completed_calls = 0;
    let mut failed_calls = 0;
    for tc in tool_calls {
        if cancel.is_cancelled() {
            let message = append_tool_err(transcript, tc, "Task cancelled");
            if let Some(ev) = tool_consumer
                .tool_result_committed(tc.tool_call_index, message)
                .await
            {
                output_events.push(ev);
            }
            completed_calls += 1;
            failed_calls += 1;
            continue;
        }

        // Notify TUI: tool started
        if let Some(ev) = tool_consumer
            .tool_started(tc.id.clone(), tc.name.clone(), tc.arguments.clone())
            .await
        {
            output_events.push(ev);
        }

        if is_spawn_tool(&tc.name) {
            let result = execute_spawn_tool(
                spawner,
                tool_consumer.agent_id(),
                tool_consumer.task_id(),
                &tool_consumer.host_context().cloned(),
                &tc.name,
                &tc.arguments,
                tool_consumer.senders(),
            )
            .await;
            let (tool_result, is_error) = match result {
                Ok(value) => (value, false),
                Err(err) => (serde_json::Value::String(err), true),
            };
            if is_error {
                failed_calls += 1;
            }

            if let Some(ev) = tool_consumer
                .tool_ended(
                    tc.id.clone(),
                    tc.name.clone(),
                    tool_result.clone(),
                    is_error,
                )
                .await
            {
                output_events.push(ev);
            }

            let message = append_tool_value(transcript, tc, tool_result, is_error);
            if let Some(ev) = tool_consumer
                .tool_result_committed(tc.tool_call_index, message)
                .await
            {
                output_events.push(ev);
            }
            completed_calls += 1;
            continue;
        }

        let r = match routes.get(&tc.name) {
            Some(r) => r,
            None => {
                if let Some(ev) = tool_consumer
                    .tool_ended(
                        tc.id.clone(),
                        tc.name.clone(),
                        serde_json::Value::Null,
                        true,
                    )
                    .await
                {
                    output_events.push(ev);
                }
                let message = append_tool_err(
                    transcript,
                    tc,
                    &format!("No route for tool \"{}\"", tc.name),
                );
                if let Some(ev) = tool_consumer
                    .tool_result_committed(tc.tool_call_index, message)
                    .await
                {
                    output_events.push(ev);
                }
                completed_calls += 1;
                failed_calls += 1;
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
            agent_id: tool_consumer.agent_id().to_string(),
            task_id: tool_consumer.task_id().to_string(),
            tool_set_ids: vec![],
            turn_index: Some(turn_index),
            event_seq: Some(0),
            next_event_seq: None,
            parent_message_id: Some(tool_consumer.parent_message_id().to_string()),
            content_index: Some(tc.content_index),
            tool_call_index: Some(tc.tool_call_index),
            tool_entity_id: Some(runtime_tool_entity_id(
                tool_consumer.parent_message_id(),
                tc.tool_call_index,
            )),
            host_context: tool_consumer.host_context().cloned(),
            senders: tool_consumer.senders().clone(),
        };
        let rec = (*deps.tool_registry)
            .execute_tool(&call, &ctx, r, Some(cancel.clone()))
            .await;

        // Notify TUI: tool ended
        if let Some(ev) = tool_consumer
            .tool_ended(
                tc.id.clone(),
                tc.name.clone(),
                rec.result.value.clone().unwrap_or(serde_json::Value::Null),
                !rec.result.ok,
            )
            .await
        {
            output_events.push(ev);
        }
        if !rec.result.ok {
            failed_calls += 1;
        }

        let message = append_tool(transcript, tc, &rec.result);
        if let Some(ev) = tool_consumer
            .tool_result_committed(tc.tool_call_index, message)
            .await
        {
            output_events.push(ev);
        }
        completed_calls += 1;
    }
    Ok(ToolExecutionResult {
        events: output_events,
        completed_calls,
        failed_calls,
    })
}

// ---- Append helpers ----

fn append_tool_value(
    transcript: &mut TranscriptManager,
    tc: &ToolCallItem,
    value: serde_json::Value,
    is_error: bool,
) -> Message {
    let text = if value.is_string() {
        value.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(&value).unwrap_or_default()
    };
    let msg = Message::ToolResult {
        tool_call_id: tc.id.clone(),
        tool_name: Some(tc.name.clone()),
        content: vec![ContentBlock::Text { text }],
        details: Some(value),
        is_error: Some(is_error),
        timestamp: None,
    };
    transcript.push_message(msg.clone());
    msg
}

fn append_tool(
    transcript: &mut TranscriptManager,
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
    transcript.push_message(msg.clone());
    msg
}

async fn execute_spawn_tool(
    spawner: &std::sync::Arc<dyn AgentSpawner>,
    source_agent_id: &str,
    parent_task_id: &str,
    host_context: &Option<crate::domain::tasks::task::HostTaskContext>,
    tool_name: &str,
    args: &serde_json::Value,
    senders: &Option<crate::runtime::dispatch::DispatchSenders>,
) -> Result<serde_json::Value, String> {
    let hc = host_context
        .clone()
        .unwrap_or_else(|| crate::domain::tasks::task::HostTaskContext {
            session_id: String::new(),
            turn_id: String::new(),
        });
    let agent_id = args
        .get("agent_id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
        .unwrap_or("general");
    let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

    match tool_name {
        "spawn" => match spawner
            .spawn(
                agent_id,
                prompt,
                Some(source_agent_id.to_string()),
                Some(parent_task_id.to_string()),
                hc,
                senders.clone(),
            )
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
                .spawn_detached(
                    agent_id,
                    prompt,
                    Some(source_agent_id.to_string()),
                    Some(parent_task_id.to_string()),
                    hc,
                    senders.clone(),
                )
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

fn append_tool_err(transcript: &mut TranscriptManager, tc: &ToolCallItem, error: &str) -> Message {
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
    transcript.push_message(msg.clone());
    msg
}
