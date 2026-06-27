// ---- AgentActor: step runner — model step + outcome processing ----

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use tokio_stream::StreamExt;

use crate::model::types::{
    ModelSpec, ModelStepEvent, ModelStepInput, ModelStepResult, runtime_assistant_message_id,
};
use crate::protocol::messages::{ContentBlock, Message};
use crate::protocol::model::{ModelProviderConfig, ModelRunSettings};
use crate::protocol::tools::ToolDef;
use crate::tools::registry::CatalogRoute;
use piko_protocol::{Event, MessageRole};

use super::tool_executor;
use super::types::*;

// Re-export
pub use crate::model::executor::ModelStepExecutor;

/// Call the model executor with the current transcript, stream deltas via emit.
#[tracing::instrument(skip(state, worker, deps, model_config, tools, cancel), fields(task_id = %task_id, step))]
pub async fn run_model_step(
    state: &Arc<Mutex<AgentRuntimeState>>,
    worker: &mut AgentWorkerState,
    deps: &AgentActorDeps,
    model_config: &Option<ModelConfig>,
    tools: &[ToolDef],
    task_id: &str,
    cancel: CancellationToken,
) -> Result<(ModelStepResult, String), anyhow::Error> {
    let (model, provider, settings) = model_config
        .as_ref()
        .map(|c| {
            let s = c.settings.clone();
            (c.model.clone(), c.provider.clone(), s)
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

    let input = ModelStepInput {
        run_id: task_id.to_string(),
        step_id: format!("step_{}", worker.step_count),
        transcript: worker.transcript.clone(),
        system_prompt: {
            let s = state.lock().await;
            s.spec.system_prompt.clone()
        },
        model,
        provider,
        settings,
        tools: tools.to_vec(),
        engine_state: worker.engine_state.clone(),
    };

    let mut stream = deps
        .model_executor
        .execute_step(input, Some(cancel.clone()));

    let agent_id = {
        let s = state.lock().await;
        s.spec.id.clone()
    };

    let step_idx = worker.step_count;
    let assistant_id = runtime_assistant_message_id(task_id, &format!("step_{}", step_idx));
    let mut cap_id = assistant_id.clone();

    // Iterate stream events
    while let Some(event) = stream.next().await {
        if cancel.is_cancelled() {
            break;
        }

        match &event {
            ModelStepEvent::MessageDelta { message_id, delta } => {
                emit_host(
                    deps,
                    Event::TextDelta {
                        task_id: task_id.to_string(),
                        agent_id: agent_id.clone(),
                        message_id: message_id.clone(),
                        delta: delta.clone(),
                    },
                )
                .await;
            }
            ModelStepEvent::ThinkingDelta { message_id, delta } => {
                emit_host(
                    deps,
                    Event::ThinkingDelta {
                        task_id: task_id.to_string(),
                        agent_id: agent_id.clone(),
                        message_id: message_id.clone(),
                        delta: delta.clone(),
                    },
                )
                .await;
            }
            ModelStepEvent::MessageStart { message } => {
                let msg_id = get_runtime_msg_id(message);
                cap_id = msg_id.clone();
                emit_host(
                    deps,
                    Event::MessageStart {
                        task_id: task_id.to_string(),
                        agent_id: agent_id.clone(),
                        message_id: msg_id,
                        role: MessageRole::Assistant,
                    },
                )
                .await;
            }
            ModelStepEvent::MessageUpdate {
                message,
                assistant_event,
            } => {
                let msg_id = get_runtime_msg_id(message);
                if let Some(ae) = assistant_event {
                    match ae {
                        crate::protocol::runtime_stream::RuntimeAssistantMessageEvent::TextDelta { delta, .. } => {
                            emit_host(deps, Event::TextDelta {
                                task_id: task_id.to_string(),
                                agent_id: agent_id.clone(),
                                message_id: msg_id,
                                delta: delta.to_string(),
                            }).await;
                        }
                        crate::protocol::runtime_stream::RuntimeAssistantMessageEvent::ThinkingDelta { delta, .. } => {
                            emit_host(deps, Event::ThinkingDelta {
                                task_id: task_id.to_string(),
                                agent_id: agent_id.clone(),
                                message_id: msg_id,
                                delta: delta.to_string(),
                            }).await;
                        }
                        _ => {}
                    }
                }
            }
            ModelStepEvent::MessageEnd { message } => {
                let msg_id = get_runtime_msg_id(message);
                let stop_reason = match message {
                    crate::protocol::runtime_stream::RuntimeMessage::Assistant {
                        stop_reason,
                        ..
                    } => stop_reason.clone().unwrap_or_else(|| "stop".to_string()),
                    _ => "stop".to_string(),
                };
                emit_host(
                    deps,
                    Event::MessageEnd {
                        task_id: task_id.to_string(),
                        agent_id: agent_id.clone(),
                        message_id: msg_id,
                        stop_reason: Some(stop_reason),
                    },
                )
                .await;
            }
            ModelStepEvent::StepStart | ModelStepEvent::StepEnd | ModelStepEvent::Error { .. } => {}
            ModelStepEvent::ProviderToolCallDelta { .. } => {}
        }
    }

    if cancel.is_cancelled() {
        return Ok((
            ModelStepResult {
                status: "aborted".into(),
                appended_messages: vec![],
                transcript_delta: vec![],
                stop_reason: "abort".into(),
                error_message: None,
                usage: None,
                engine_state: worker.engine_state.clone(),
            },
            assistant_id,
        ));
    }

    let step_result = match stream.result().await {
        Ok(r) => r,
        Err(e) => {
            return Ok((
                ModelStepResult {
                    status: "error".into(),
                    appended_messages: vec![],
                    transcript_delta: vec![],
                    stop_reason: "error".into(),
                    error_message: Some(e.to_string()),
                    usage: None,
                    engine_state: worker.engine_state.clone(),
                },
                assistant_id,
            ));
        }
    };

    // Merge messages into transcript
    for msg in &step_result.appended_messages {
        if !worker.transcript.contains(msg) {
            worker.transcript.push(msg.clone());
        }
    }

    worker.engine_state = step_result.engine_state.clone();

    Ok((step_result, cap_id))
}

/// Process the step result: check terminal conditions, extract tool calls, execute.
pub async fn process_step_outcome(
    state: &Arc<Mutex<AgentRuntimeState>>,
    worker: &mut AgentWorkerState,
    deps: &AgentActorDeps,
    task_id: &str,
    step_result: &ModelStepResult,
    model_settings: &ModelRunSettings,
    routes: &HashMap<String, CatalogRoute>,
    cancel: CancellationToken,
    assistant_message_id: &str,
) -> StepOutcome {
    if step_result.status == "error" || step_result.status == "aborted" || cancel.is_cancelled() {
        let final_status = if cancel.is_cancelled() || step_result.status == "aborted" {
            "aborted"
        } else {
            "error"
        };
        let summary = if step_result.status == "error" {
            step_result
                .error_message
                .as_deref()
                .unwrap_or("Unknown engine error")
        } else {
            "Task cancelled"
        };
        return StepOutcome::Terminal {
            result: AgentTaskResultExt {
                summary: summary.into(),
                messages: worker.transcript.clone(),
                total_steps: worker.step_count,
                final_status: final_status.into(),
                artifacts: None,
            },
        };
    }

    // Extract text from assistant messages
    let text_content = step_result
        .appended_messages
        .iter()
        .filter_map(|m| match m {
            Message::Assistant { content, .. } => content.iter().find_map(|b| match b {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            }),
            _ => None,
        })
        .next();

    // Extract tool calls
    let tool_calls: Vec<tool_executor::ToolCallItem> = {
        let mut tcs = Vec::new();
        let mut tc_idx = 0u32;
        for msg in &step_result.appended_messages {
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
                final_status: if cancel.is_cancelled() {
                    "aborted".into()
                } else {
                    "completed".into()
                },
                artifacts: None,
            },
        };
    }

    if cancel.is_cancelled() {
        return StepOutcome::Terminal {
            result: AgentTaskResultExt {
                summary: "Task cancelled".into(),
                messages: worker.transcript.clone(),
                total_steps: worker.step_count,
                final_status: "aborted".into(),
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

// ---- Helpers ----

/// Extract the ID string from a RuntimeMessage enum.
fn get_runtime_msg_id(msg: &crate::protocol::runtime_stream::RuntimeMessage) -> String {
    match msg {
        crate::protocol::runtime_stream::RuntimeMessage::User { id, .. } => id.clone(),
        crate::protocol::runtime_stream::RuntimeMessage::Assistant { id, .. } => id.clone(),
        crate::protocol::runtime_stream::RuntimeMessage::ToolResult { id, .. } => id.clone(),
        crate::protocol::runtime_stream::RuntimeMessage::Custom { id, .. } => id.clone(),
    }
}

async fn emit_host(deps: &AgentActorDeps, event: Event) {
    let val = serde_json::to_value(&event).unwrap_or_default();
    (deps.emit_fn)(String::new(), val).await;
}
