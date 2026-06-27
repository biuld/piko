// ---- AgentActor: tool executor — parallel / sequential execution ----

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::protocol::messages::{ContentBlock, Message};
use crate::protocol::model::ModelRunSettings;
use crate::protocol::tools::ToolExecutionMode;
use crate::tools::registry::{CatalogRoute, ToolRegistry};
use crate::tools::{ToolExecError, ToolExecResult, ToolExecutionContext};

use super::types::*;

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

// ---- Public API ----

/// Execute a batch of tool calls. Resolves parallel vs sequential automatically.
#[tracing::instrument(skip(state, worker, deps, tool_calls, routes, cancel), fields(task_id = %task_id, tool_count = tool_calls.len()))]
pub async fn execute_tool_calls(
    state: &Arc<Mutex<AgentRuntimeState>>,
    worker: &mut AgentWorkerState,
    deps: &AgentActorDeps,
    task_id: &str,
    tool_calls: &[ToolCallItem],
    model_settings: &ModelRunSettings,
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    parent_message_id: &str,
) {
    let mode = resolve_execution_mode(tool_calls, routes, model_settings);

    if mode == ExecutionMode::Parallel {
        execute_parallel(
            state,
            worker,
            deps,
            task_id,
            tool_calls,
            routes,
            cancel,
            parent_message_id,
        )
        .await;
    } else {
        execute_sequential(
            state,
            worker,
            deps,
            task_id,
            tool_calls,
            routes,
            cancel,
            parent_message_id,
        )
        .await;
    }
}

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

async fn get_agent_id(state: &Arc<Mutex<AgentRuntimeState>>) -> String {
    state.lock().await.spec.id.clone()
}

// ---- Parallel execution ----

async fn execute_parallel(
    state: &Arc<Mutex<AgentRuntimeState>>,
    worker: &mut AgentWorkerState,
    deps: &AgentActorDeps,
    task_id: &str,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    parent_message_id: &str,
) {
    let agent_id = get_agent_id(state).await;
    let host_context = state.lock().await.current_host_context.clone();
    let turn_index = worker.step_count;

    let mut handles = Vec::new();

    for tc in tool_calls.iter() {
        let route = routes.get(&tc.name).cloned();
        let deps = deps.clone();
        let agent_id = agent_id.clone();
        let host_context = host_context.clone();
        let task_id = task_id.to_string();
        let tc = tc.clone();
        let pm_id = parent_message_id.to_string();
        let cancel = cancel.clone();

        let handle = tokio::spawn(async move {
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
                );
            }

            match route {
                Some(r) => {
                    let call = ContentBlock::ToolCall {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                        partial_json: None,
                    };
                    let exec_ctx = ToolExecutionContext {
                        agent_id: agent_id.clone(),
                        task_id: task_id.clone(),
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

                    let result = (*deps.tool_registry)
                        .execute_tool(&call, &exec_ctx, &r, Some(cancel.clone()))
                        .await;

                    // execute_tool returns ToolExecResult directly
                    (tc.clone(), result)
                }
                None => {
                    let error_result = ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "not_found".into(),
                            message: format!("No route for tool \"{}\"", tc.name),
                            retryable: Some(false),
                        }),
                    };
                    (tc.clone(), error_result)
                }
            }
        });

        handles.push(handle);
    }

    let mut results: Vec<(ToolCallItem, ToolExecResult)> = Vec::new();
    for h in handles {
        if let Ok((tc, result)) = h.await {
            results.push((tc, result));
        }
    }

    results.sort_by_key(|(tc, _)| tc.tool_call_index);

    for (tc, exec_result) in results {
        append_tool_result(worker, &tc, &exec_result);
    }
}

// ---- Sequential execution ----

async fn execute_sequential(
    state: &Arc<Mutex<AgentRuntimeState>>,
    worker: &mut AgentWorkerState,
    deps: &AgentActorDeps,
    task_id: &str,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    parent_message_id: &str,
) {
    let agent_id = get_agent_id(state).await;
    let host_context = state.lock().await.current_host_context.clone();
    let turn_index = worker.step_count;

    for tc in tool_calls {
        if cancel.is_cancelled() {
            append_tool_error(worker, tc, "Task cancelled");
            continue;
        }

        let route = match routes.get(&tc.name) {
            Some(r) => r,
            None => {
                append_tool_error(worker, tc, &format!("No route for tool \"{}\"", tc.name));
                continue;
            }
        };

        let call = ContentBlock::ToolCall {
            id: tc.id.clone(),
            name: tc.name.clone(),
            arguments: tc.arguments.clone(),
            partial_json: None,
        };

        let exec_ctx = ToolExecutionContext {
            agent_id: agent_id.clone(),
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

        let result = (*deps.tool_registry)
            .execute_tool(&call, &exec_ctx, route, Some(cancel.clone()))
            .await;

        append_tool_result(worker, tc, &result);
    }
}

// ---- Append helpers ----

fn append_tool_result(worker: &mut AgentWorkerState, tc: &ToolCallItem, result: &ToolExecResult) {
    if result.ok {
        let text = match &result.value {
            Some(v) if v.is_string() => v.as_str().unwrap_or("").to_string(),
            Some(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
            None => String::new(),
        };

        worker.transcript.push(Message::ToolResult {
            tool_call_id: tc.id.clone(),
            tool_name: Some(tc.name.clone()),
            content: vec![ContentBlock::Text { text }],
            details: result.value.clone(),
            is_error: Some(false),
            timestamp: None,
        });
    } else {
        let error_msg = result
            .error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_else(|| "Unknown error".into());

        worker.transcript.push(Message::ToolResult {
            tool_call_id: tc.id.clone(),
            tool_name: Some(tc.name.clone()),
            content: vec![ContentBlock::Text { text: error_msg }],
            details: result
                .error
                .as_ref()
                .map(|e| serde_json::to_value(e).unwrap_or_default()),
            is_error: Some(true),
            timestamp: None,
        });
    }
}

fn append_tool_error(worker: &mut AgentWorkerState, tc: &ToolCallItem, error: &str) {
    worker.transcript.push(Message::ToolResult {
        tool_call_id: tc.id.clone(),
        tool_name: Some(tc.name.clone()),
        content: vec![ContentBlock::Text {
            text: format!("Tool error: {error}"),
        }],
        details: Some(serde_json::json!({"error": error})),
        is_error: Some(true),
        timestamp: None,
    });
}
