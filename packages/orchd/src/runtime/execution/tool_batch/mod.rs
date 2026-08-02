//! F-06 tool batch dispatch: grouping, concurrency, and cancellation.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::future::join_all;
use piko_protocol::Message;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry, ToolRegistryImpl};
use crate::domain::tools::call::{ToolCall, ToolCallItem};
use crate::domain::tools::definition::ToolExecutionMode;
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::ToolExecutionContext;
use crate::runtime::utils::runtime_tool_entity_id;

use super::ExecutionIdentity;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;

/// A run of consecutive tool calls sharing the same effective execution mode.
#[derive(Debug, Clone)]
pub(super) struct ToolRunGroup<'a> {
    pub(super) mode: ToolExecutionMode,
    pub(super) calls: Vec<&'a ToolCallItem>,
}

/// Group a step's tool calls by consecutive effective execution mode.
///
/// Parallel calls that are not separated by a sequential call overlap; a
/// sequential call always runs exclusively. Unknown tools default to
/// `Sequential` (fail-closed) and occupy their own transcript slot.
pub(super) fn group_tool_calls<'a>(
    tool_calls: &'a [ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
) -> Vec<ToolRunGroup<'a>> {
    let mut groups: Vec<ToolRunGroup<'a>> = Vec::new();
    for tc in tool_calls {
        let mode = routes
            .get(&tc.name)
            .map(|route| route.execution_mode.clone())
            .unwrap_or(ToolExecutionMode::Sequential);
        match groups.last_mut() {
            Some(last) if last.mode == mode => last.calls.push(tc),
            _ => groups.push(ToolRunGroup {
                mode,
                calls: vec![tc],
            }),
        }
    }
    groups
}

/// Build the durable `ToolCall` message the runtime commits for a call.
pub(super) fn tool_call_message(tc: &ToolCallItem) -> Message {
    Message::ToolCall {
        id: tc.id.clone(),
        name: tc.name.clone(),
        arguments: tc.arguments.clone(),
        model: None,
        provider: None,
        timestamp: Some(chrono::Utc::now().timestamp_millis()),
    }
}

pub(super) fn tool_call_message_id(parent_message_id: &str, tool_call_index: u32) -> String {
    format!("{parent_message_id}:tool_call:{tool_call_index}")
}

pub(super) fn tool_result_message_id(parent_message_id: &str, tool_call_index: u32) -> String {
    format!("{parent_message_id}:tool_result:{tool_call_index}")
}

/// Bounded error result committed for a call aborted by cancellation.
pub(super) fn aborted_tool_exec_result() -> ToolExecResult {
    ToolExecResult {
        ok: false,
        value: None,
        error: Some(ToolExecError {
            code: "aborted".into(),
            message: "Task cancelled".into(),
            retryable: Some(false),
        }),
    }
}

/// Build the shared execution context for a tool call in this execution.
fn tool_exec_context(
    identity: &ExecutionIdentity,
    cancel: CancellationToken,
    model_step_index: u32,
    tc: &ToolCallItem,
    parent_message_id: &str,
) -> ToolExecutionContext {
    ToolExecutionContext {
        session_id: identity.session_id.clone(),
        agent_instance_id: identity.agent_instance_id.clone(),
        execution_id: identity.execution_id.clone(),
        cancellation: Some(cancel),
        agent_id: identity.agent_id.clone(),
        tool_set_ids: vec![],
        turn_index: Some(model_step_index),
        event_seq: Some(0),
        next_event_seq: None,
        parent_message_id: Some(parent_message_id.to_string()),
        content_index: Some(tc.content_index),
        tool_call_index: Some(tc.tool_call_index),
        tool_entity_id: Some(runtime_tool_entity_id(
            parent_message_id,
            tc.tool_call_index,
        )),
        host_context: Some(piko_protocol::agents::HostSessionContext::new(
            identity.session_id.clone(),
        )),
        source_turn_id: identity.source_turn_id.clone(),
    }
}

/// Execute one tool call exclusively through the registry, aborting it when
/// the run is cancelled so a bounded aborted result can be committed.
pub(super) async fn execute_sequential_call(
    registry: Arc<ToolRegistryImpl>,
    cancel: CancellationToken,
    model_step_index: u32,
    identity: &ExecutionIdentity,
    tc: &ToolCallItem,
    route: &CatalogRoute,
    parent_message_id: &str,
) -> ToolExecResult {
    let call = ToolCall {
        id: tc.id.clone(),
        name: tc.name.clone(),
        arguments: tc.arguments.clone(),
        partial_json: None,
    };
    let exec_ctx = tool_exec_context(
        identity,
        cancel.clone(),
        model_step_index,
        tc,
        parent_message_id,
    );
    let cancel_for_exec = cancel.clone();
    let execute = async move {
        registry
            .execute_tool(&call, &exec_ctx, route, Some(cancel_for_exec))
            .await
            .result
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => aborted_tool_exec_result(),
        record = execute => record,
    }
}

/// Execute a parallel group: run all calls concurrently under a semaphore
/// (each future races cancellation) and return results in call order.
pub(super) async fn execute_parallel_group(
    registry: Arc<ToolRegistryImpl>,
    cancel: CancellationToken,
    model_step_index: u32,
    identity: &ExecutionIdentity,
    calls: &[&ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    parent_message_id: &str,
) -> Vec<ToolExecResult> {
    // Concurrency cap: min of the declared set-level caps (0 treated as 1),
    // bounded by the group size; unset means the whole group may overlap.
    let group_size = calls.len() as u32;
    let cap = calls
        .iter()
        .filter_map(|tc| routes.get(&tc.name))
        .filter_map(|route| route.max_concurrent_calls)
        .map(|declared| declared.max(1).min(group_size))
        .min()
        .unwrap_or(group_size) as usize;
    let semaphore = Arc::new(Semaphore::new(cap));
    let contexts: Vec<ToolExecutionContext> = calls
        .iter()
        .map(|tc| {
            tool_exec_context(
                identity,
                cancel.clone(),
                model_step_index,
                tc,
                parent_message_id,
            )
        })
        .collect();

    let futures: Vec<_> = calls
        .iter()
        .zip(contexts)
        .map(|(tc, exec_ctx)| {
            let semaphore = Arc::clone(&semaphore);
            let cancel = cancel.clone();
            let registry = Arc::clone(&registry);
            let route = routes.get(&tc.name).cloned();
            let call = ToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
                partial_json: None,
            };
            async move {
                let Some(route) = route else {
                    return ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "not_found".into(),
                            message: format!("No route for tool \"{}\"", call.name),
                            retryable: Some(false),
                        }),
                    };
                };
                let cancel_for_exec = cancel.clone();
                let execute = async move {
                    let _permit = semaphore
                        .acquire_owned()
                        .await
                        .expect("batch semaphore closed");
                    registry
                        .execute_tool(&call, &exec_ctx, &route, Some(cancel_for_exec))
                        .await
                        .result
                };
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => aborted_tool_exec_result(),
                    record = execute => record,
                }
            }
        })
        .collect();

    join_all(futures).await
}
