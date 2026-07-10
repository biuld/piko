use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry};
use crate::domain::model::transcript::TranscriptManager;
use crate::domain::tools::call::ToolCall;
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::runtime::dispatch::ToolExecutionConsumer;
use crate::runtime::orchestrator::AgentRunDeps;
use crate::runtime::types::ToolCallItem;

use super::ToolExecutionResult;
use super::transcript::append_tool;

pub(super) async fn execute_parallel_direct(
    deps: &AgentRunDeps,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    transcript: &mut TranscriptManager,
    turn_index: u32,
    tool_consumer: &ToolExecutionConsumer,
) -> Result<ToolExecutionResult, String> {
    let mut futures = Vec::new();
    for tc in tool_calls {
        let route = routes.get(&tc.name).cloned();
        let tool_consumer = tool_consumer.clone();
        let (deps, tc, cancel) = (deps.clone(), tc.clone(), cancel.clone());
        futures.push(async move {
            let tool_consumer = tool_consumer;

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
                    Vec::new(),
                );
            }

            tool_consumer.emit_tool_started(&tc).await;

            if let Some(r) = route {
                let call = ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    partial_json: None,
                };
                let exec_ctx = tool_consumer.tool_execution_context(turn_index, &tc);
                let rec = (*deps.tool_registry)
                    .execute_tool(&call, &exec_ctx, &r, Some(cancel.clone()))
                    .await;

                let result_value = rec.result.value.clone().unwrap_or(serde_json::Value::Null);
                tool_consumer
                    .emit_tool_ended(&tc, &result_value, !rec.result.ok)
                    .await;

                (tc.clone(), rec.result, Vec::new())
            } else {
                let result_value = serde_json::Value::Null;
                tool_consumer
                    .emit_tool_ended(&tc, &result_value, true)
                    .await;
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
                    Vec::new(),
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
        let consumer = tool_consumer.clone();
        let result_message_id = consumer.tool_result_message_id(tc.tool_call_index);
        consumer
            .emit_tool_result_committed(&message, &result_message_id)
            .await;
    }
    Ok(ToolExecutionResult {
        events: output_events,
        completed_calls: tool_calls.len(),
        failed_calls,
    })
}
