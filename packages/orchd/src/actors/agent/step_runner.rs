// ---- AgentActor: step runner — model step + outcome processing ----

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tokio_stream::StreamExt;

use llmd::gateway::{GatewayEvent, GatewayRequest};
use piko_protocol::messages::{ContentBlock, Message};
use piko_protocol::model::{ModelProviderConfig, ModelRunSettings};
use piko_protocol::tools::ToolDef;
use piko_protocol::{Event, MessageRole};

use super::tool_executor;
use super::types::*;

/// Call the model as a streaming gateway and emit UI events chunk-by-chunk.
///
/// Returns the final assembled assistant Message (with tool calls and usage).
#[tracing::instrument(skip(state, worker, deps, model_config, tools, cancel), fields(task_id = %task_id, step))]
pub async fn run_model_step(
    state: &Arc<Mutex<AgentRuntimeState>>,
    worker: &mut AgentWorkerState,
    deps: &AgentActorDeps,
    model_config: &Option<ModelConfig>,
    tools: &[ToolDef],
    task_id: &str,
    cancel: CancellationToken,
) -> Result<Message, anyhow::Error> {
    let (model, _provider, _settings) = model_config
        .as_ref()
        .map(|c| {
            (c.model.clone(), c.provider.clone(), c.settings.clone())
        })
        .unwrap_or_else(|| {
            (
                ModelSpec {
                    id: "default".into(),
                    name: "Default".into(),
                    provider: "openai".into(),
                },
                ModelProviderConfig::default(),
                ModelRunSettings {
                    allow_tool_calls: true,
                    ..Default::default()
                },
            )
        });

    let agent_id = {
        let s = state.lock().await;
        s.spec.id.clone()
    };

    let step_idx = worker.step_count;
    let assistant_id = runtime_assistant_message_id(task_id, &format!("step_{}", step_idx));

    let request = GatewayRequest {
        run_id: task_id.to_string(),
        step_id: format!("step_{}", worker.step_count),
        transcript: worker.transcript.clone(),
        system_prompt: {
            let s = state.lock().await;
            s.spec.system_prompt.clone()
        },
        model: model.id.clone(),
        provider: model.provider.clone(),
        tools: tools.to_vec(),
        thinking: model_config.as_ref().and_then(|c| c.resolve_thinking()),
    };

    // 1. Signal to UI: assistant is starting to think
    emit_host(
        deps,
        Event::MessageStart {
            task_id: task_id.to_string(),
            agent_id: agent_id.clone(),
            message_id: assistant_id.clone(),
            role: MessageRole::Assistant,
        },
    )
    .await;

    // 2. Call the gateway — this awaits the HTTP connection setup,
    //    then returns a Stream that yields chunks as they arrive.
    let mut stream = match deps.model_executor.chat_stream(request, Some(cancel.clone())).await {
        Ok(s) => s,
        Err(e) => {
            emit_host(deps, Event::MessageEnd {
                task_id: task_id.to_string(),
                agent_id: agent_id.clone(),
                message_id: assistant_id.clone(),
                stop_reason: Some("error".into()),
            }).await;
            return Err(anyhow::anyhow!("Gateway error: {e}"));
        }
    };

    let mut current_text = String::new();
    let mut current_reasoning = String::new();
    let mut tool_calls_map: HashMap<usize, (String, String, serde_json::Value)> = HashMap::new();
    let mut final_usage = None;
    let mut final_stop_reason = "stop".to_string();

    // 3. Consume the stream — each chunk is emitted as a UI delta event
    while let Some(event) = stream.next().await {
        if cancel.is_cancelled() {
            final_stop_reason = "abort".to_string();
            break;
        }

        match event {
            GatewayEvent::ContentDelta(delta) => {
                current_text.push_str(&delta);
                emit_host(
                    deps,
                    Event::TextDelta {
                        task_id: task_id.to_string(),
                        agent_id: agent_id.clone(),
                        message_id: assistant_id.clone(),
                        delta,
                    },
                )
                .await;
            }
            GatewayEvent::ReasoningDelta(delta) => {
                current_reasoning.push_str(&delta);
                emit_host(
                    deps,
                    Event::ThinkingDelta {
                        task_id: task_id.to_string(),
                        agent_id: agent_id.clone(),
                        message_id: assistant_id.clone(),
                        delta,
                    },
                )
                .await;
            }
            GatewayEvent::ToolCallStart { index, id, name, args } => {
                tool_calls_map.insert(index, (id, name, args));
            }
            GatewayEvent::Usage(usage) => {
                final_usage = Some(usage);
            }
            GatewayEvent::Done(reason) => {
                final_stop_reason = reason;
            }
            GatewayEvent::Error(e) => {
                final_stop_reason = "error".to_string();
                tracing::error!("Stream error: {e}");
                break;
            }
        }
    }

    emit_host(
        deps,
        Event::MessageEnd {
            task_id: task_id.to_string(),
            agent_id: agent_id.clone(),
            message_id: assistant_id.clone(),
            stop_reason: Some(final_stop_reason.clone()),
        },
    )
    .await;

    // 4. Assemble the final protocol Message from accumulated chunks
    let mut blocks = Vec::new();

    if !current_reasoning.is_empty() {
        blocks.push(ContentBlock::Thinking {
            thinking: current_reasoning,
            thinking_signature: None,
        });
    }

    if !current_text.is_empty() {
        blocks.push(ContentBlock::Text {
            text: current_text,
        });
    }

    // Sort tool calls by index to preserve provider ordering
    let mut sorted_indices: Vec<_> = tool_calls_map.keys().copied().collect();
    sorted_indices.sort_unstable();

    for idx in sorted_indices {
        let (id, name, args) = tool_calls_map.remove(&idx).unwrap();
        blocks.push(ContentBlock::ToolCall {
            id,
            name,
            arguments: args,
            partial_json: None,
        });
    }

    if blocks.is_empty() {
        blocks.push(ContentBlock::Text {
            text: String::new(),
        });
    }

    let final_message = Message::Assistant {
        content: blocks,
        api: "openai-completions".into(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        usage: final_usage,
        stop_reason: Some(final_stop_reason),
        error_message: None,
        timestamp: Some(chrono::Utc::now().timestamp_millis()),
    };

    Ok(final_message)
}

/// Process the step result: check terminal conditions, extract tool calls, execute.
pub async fn process_step_outcome(
    state: &Arc<Mutex<AgentRuntimeState>>,
    worker: &mut AgentWorkerState,
    deps: &AgentActorDeps,
    task_id: &str,
    message: Option<&Message>,
    error_message: Option<String>,
    model_settings: &ModelRunSettings,
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    assistant_message_id: &str,
) -> StepOutcome {
    if error_message.is_some() || cancel.is_cancelled() {
        let final_status = if cancel.is_cancelled() {
            "aborted"
        } else {
            "error"
        };
        let summary = error_message.unwrap_or_else(|| "Task cancelled".to_string());
        return StepOutcome::Terminal {
            result: AgentTaskResultExt {
                summary,
                messages: worker.transcript.clone(),
                total_steps: worker.step_count,
                final_status: final_status.into(),
                artifacts: None,
            },
        };
    }

    let msg = match message {
        Some(m) => m,
        None => {
            return StepOutcome::Terminal {
                result: AgentTaskResultExt {
                    summary: "No message returned from model step".into(),
                    messages: worker.transcript.clone(),
                    total_steps: worker.step_count,
                    final_status: "error".into(),
                    artifacts: None,
                },
            }
        }
    };

    // Extract text from assistant message
    let text_content = match msg {
        Message::Assistant { content, .. } => content.iter().find_map(|b| match b {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        }),
        _ => None,
    };

    // Extract tool calls
    let tool_calls: Vec<tool_executor::ToolCallItem> = {
        let mut tcs = Vec::new();
        let mut tc_idx = 0u32;
        if let Message::Assistant { content, .. } = msg {
            for (i, block) in content.iter().enumerate() {
                if let ContentBlock::ToolCall {
                    id,
                    name,
                    arguments,
                    ..
                } = block
                {
                    tcs.push(tool_executor::ToolCallItem {
                        content_index: i as u32,
                        tool_call_index: tc_idx,
                        id: id.clone(),
                        name: name.clone(),
                        arguments: arguments.clone(),
                    });
                    tc_idx += 1;
                }
            }
        }
        tcs
    };

    if tool_calls.is_empty() || !model_settings.allow_tool_calls {
        let summary = text_content.unwrap_or_default();
        let short_summary = if summary.len() > 200 {
            format!("{}...", &summary[..200])
        } else {
            summary
        };
        return StepOutcome::Terminal {
            result: AgentTaskResultExt {
                summary: short_summary,
                messages: worker.transcript.clone(),
                total_steps: worker.step_count,
                final_status: "completed".into(),
                artifacts: None,
            },
        };
    }

    tool_executor::execute_tool_calls(
        state,
        worker,
        deps,
        task_id,
        &tool_calls,
        model_settings,
        routes,
        cancel,
        assistant_message_id,
    )
    .await;

    StepOutcome::Continue
}

use crate::tools::registry::CatalogRoute;

async fn emit_host(deps: &AgentActorDeps, event: Event) {
    let val = serde_json::to_value(&event).unwrap_or_default();
    (deps.emit_fn)(String::new(), val).await;
}
