// ---- AgentActor: engine loop — main LLM step loop ----

use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_actors::actor::handle::ActorHandle;
use tokio_util::sync::CancellationToken;

use crate::protocol::agents::AgentTask;
use crate::protocol::host_event::HostEvent;
use crate::protocol::messages::{Message, MessageContent};
use crate::protocol::model::ModelRunSettings;
use crate::protocol::tools::ToolDiscoveryContext;
use crate::tools::registry::ToolRegistry;

use super::actor::AgentActor;
use super::step_runner::{process_step_outcome, run_model_step};
use super::types::*;

/// Spawn the engine loop in a background task.
/// The agent loop sends `RunnerFinished`/`RunnerFailed` back via `self_handle`.
///
/// Observability: `#[tracing::instrument]` records start/end with task_id & agent_id.
/// Additional detail per step is captured in `agent_loop` and `step_runner`.
#[tracing::instrument(
    skip(state, deps, self_handle, task, cancel),
    fields(
        task_id = %task.id.as_deref().unwrap_or("unknown"),
        agent_id = %task.target_agent_id
    )
)]
pub fn start_agent_run(
    state: Arc<Mutex<AgentRuntimeState>>,
    deps: AgentActorDeps,
    self_handle: ActorHandle<AgentActor>,
    task: AgentTask,
    token: u64,
    cancel: CancellationToken,
) {
    let task_id = task.id.clone().unwrap_or_default();

    tokio::spawn(async move {
        let mut transcript = task.history.clone().unwrap_or_default();

        // Append user message if not empty
        if !task.prompt.trim().is_empty() {
            transcript.push(Message::User {
                content: MessageContent::String(task.prompt.clone()),
                timestamp: None,
            });
        }

        let mut worker = AgentWorkerState {
            transcript,
            step_count: 0,
            next_message_index: 0,
            message_index_by_id: Default::default(),
            event_seq: 0,
            engine_state: None,
        };

        let result = run_agent_loop(&state, &mut worker, &deps, &task_id, cancel).await;

        match result {
            Ok(r) => {
                let _ = self_handle
                    .notify(AgentMsg::RunnerFinished {
                        task_id,
                        token,
                        result: r,
                    })
                    .await;
            }
            Err(e) => {
                let _ = self_handle
                    .notify(AgentMsg::RunnerFailed {
                        task_id,
                        token,
                        error: e.to_string(),
                    })
                    .await;
            }
        }
    });
}

/// Main model step loop: discover → call model → process outcome → repeat.
///
/// Observability: `#[tracing::instrument]` captures overall loop duration.
/// Per-step detail is in `run_model_step` and `execute_tool_calls`.
#[tracing::instrument(
    skip(state, worker, deps, cancel),
    fields(task_id = %task_id)
)]
pub async fn run_agent_loop(
    state: &Arc<Mutex<AgentRuntimeState>>,
    worker: &mut AgentWorkerState,
    deps: &AgentActorDeps,
    task_id: &str,
    cancel: CancellationToken,
) -> Result<AgentTaskResultExt, anyhow::Error> {
    let model_settings = deps
        .model_config
        .as_ref()
        .map(|c| c.settings.clone())
        .unwrap_or(ModelRunSettings {
            allow_tool_calls: true,
            ..Default::default()
        });

    let model_config = deps.model_config.clone();

    loop {
        if cancel.is_cancelled() {
            return Ok(AgentTaskResultExt {
                summary: "Task cancelled".into(),
                messages: worker.transcript.clone(),
                total_steps: worker.step_count,
                final_status: "aborted".into(),
                artifacts: None,
            });
        }

        // Drain steering queue — consume all pending steering messages before this step
        {
            let mut s = state.lock().await;
            while let Some(steering) = s.steering_queue.pop() {
                // Append steering as a user message in transcript
                worker.transcript.push(Message::User {
                    content: MessageContent::String(steering.message.clone()),
                    timestamp: None,
                });
                // Emit TaskSteered domain event
                if let Some(context) = &s.current_host_context {
                    (deps.emit_fn)(
                        String::new(),
                        serde_json::to_value(&HostEvent::TaskSteered {
                            session_id: context.session_id.clone(),
                            task_id: task_id.to_string(),
                            source_task_id: steering.source_task_id,
                            source_agent_id: steering.source_agent_id,
                            message: steering.message,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as i64,
                        })
                        .unwrap_or_default(),
                    )
                    .await;
                }
            }
        }

        worker.step_count += 1;

        // Discover tools
        let (tools, routes) = {
            let s = state.lock().await;
            (*deps.tool_registry)
                .discover_tools(&ToolDiscoveryContext {
                    agent_id: s.spec.id.clone(),
                    task_id: Some(task_id.to_string()),
                    tool_set_ids: s.spec.tool_set_ids.clone(),
                    active_tool_names: s.spec.active_tool_names.clone(),
                })
                .await
        };

        if cancel.is_cancelled() {
            return Ok(AgentTaskResultExt {
                summary: "Task cancelled".into(),
                messages: worker.transcript.clone(),
                total_steps: worker.step_count,
                final_status: "aborted".into(),
                artifacts: None,
            });
        }

        // Run model step (has its own #[tracing::instrument])
        let (step_result, assistant_message_id) = run_model_step(
            state,
            worker,
            deps,
            &model_config,
            &tools,
            task_id,
            cancel.clone(),
        )
        .await?;

        if cancel.is_cancelled() {
            return Ok(AgentTaskResultExt {
                summary: "Task cancelled".into(),
                messages: worker.transcript.clone(),
                total_steps: worker.step_count,
                final_status: "aborted".into(),
                artifacts: None,
            });
        }

        // Process outcome
        let outcome = process_step_outcome(
            state,
            worker,
            deps,
            task_id,
            &step_result,
            &model_settings,
            &routes,
            cancel.clone(),
            &assistant_message_id,
        )
        .await;

        match outcome {
            StepOutcome::Terminal { result } => return Ok(result),
            StepOutcome::Continue => continue,
        }
    }
}
