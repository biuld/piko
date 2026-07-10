use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry};
use crate::domain::model::transcript::TranscriptManager;
use crate::domain::tools::call::ToolCall;
use crate::runtime::dispatch::ToolExecutionConsumer;
use crate::runtime::orchestrator::AgentRunDeps;
use crate::runtime::types::ToolCallItem;

use super::ToolExecutionResult;
use super::transcript::{append_tool, append_tool_err};

pub(super) async fn execute_sequential_direct(
    deps: &AgentRunDeps,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    transcript: &mut TranscriptManager,
    turn_index: u32,
    tool_consumer: &ToolExecutionConsumer,
) -> Result<ToolExecutionResult, String> {
    let mut completed_calls = 0;
    let mut failed_calls = 0;
    let tool_consumer = tool_consumer.clone();
    for tc in tool_calls {
        if cancel.is_cancelled() {
            let message = append_tool_err(transcript, tc, "Task cancelled");
            let result_message_id = tool_consumer.tool_result_message_id(tc.tool_call_index);
            tool_consumer
                .emit_tool_result_committed(&message, &result_message_id)
                .await;
            completed_calls += 1;
            failed_calls += 1;
            continue;
        }

        tool_consumer.emit_tool_started(tc).await;

        let r = match routes.get(&tc.name) {
            Some(r) => r,
            None => {
                let result_value = serde_json::Value::Null;
                tool_consumer.emit_tool_ended(tc, &result_value, true).await;
                let message = append_tool_err(
                    transcript,
                    tc,
                    &format!("No route for tool \"{}\"", tc.name),
                );
                let result_message_id = tool_consumer.tool_result_message_id(tc.tool_call_index);
                tool_consumer
                    .emit_tool_result_committed(&message, &result_message_id)
                    .await;
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
        let exec_ctx = tool_consumer.tool_execution_context(turn_index, tc);
        let rec = (*deps.tool_registry)
            .execute_tool(&call, &exec_ctx, r, Some(cancel.clone()))
            .await;

        let result_value = rec.result.value.clone().unwrap_or(serde_json::Value::Null);
        tool_consumer
            .emit_tool_ended(tc, &result_value, !rec.result.ok)
            .await;
        if !rec.result.ok {
            failed_calls += 1;
        }

        let message = append_tool(transcript, tc, &rec.result);
        let result_message_id = tool_consumer.tool_result_message_id(tc.tool_call_index);
        tool_consumer
            .emit_tool_result_committed(&message, &result_message_id)
            .await;
        completed_calls += 1;
    }
    Ok(ToolExecutionResult {
        events: Vec::new(),
        completed_calls,
        failed_calls,
    })
}
