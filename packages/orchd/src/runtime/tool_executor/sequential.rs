use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry};
use crate::domain::model::transcript::TranscriptManager;
use crate::domain::tools::call::ToolCall;
use crate::ports::agent_spawner::AgentSpawner;
use crate::ports::tool_provider::ToolExecutionContext;
use crate::runtime::dispatch::ToolExecutionConsumer;
use crate::runtime::orchestrator::AgentRunDeps;
use crate::runtime::types::ToolCallItem;
use crate::runtime::utils::runtime_tool_entity_id;

use super::ToolExecutionResult;
use super::spawn::execute_spawn_tool;
use super::spawn::is_spawn_tool;
use super::transcript::{append_tool, append_tool_err, append_tool_value};

pub(super) async fn execute_sequential_direct(
    deps: &AgentRunDeps,
    spawner: &std::sync::Arc<dyn AgentSpawner>,
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

            tool_consumer
                .emit_tool_ended(tc, &tool_result, is_error)
                .await;

            let message = append_tool_value(transcript, tc, tool_result, is_error);
            let result_message_id = tool_consumer.tool_result_message_id(tc.tool_call_index);
            tool_consumer
                .emit_tool_result_committed(&message, &result_message_id)
                .await;
            completed_calls += 1;
            continue;
        }

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
        let exec_ctx = ToolExecutionContext {
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
