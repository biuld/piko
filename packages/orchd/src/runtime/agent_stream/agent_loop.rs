// ---- Agent execution: engine loop — main LLM step loop ----

use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::domain::events::event::Event;
use crate::domain::model::step::ModelRunSettings;
use crate::domain::model::transcript::{Message, MessageContent};
use crate::domain::tasks::task::AgentTask;
use crate::ports::tool_provider::ToolDiscoveryContext;

use super::messages::*;
use super::step_runner::{process_step_outcome, run_model_step};

/// Build per-run worker state and execute the agent loop.
#[tracing::instrument(
    skip(state, deps, task, cancel),
    fields(
        task_id = %task.id.as_deref().unwrap_or("unknown"),
        agent_id = %task.target_agent_id
    )
)]
pub(crate) async fn run_agent_task(
    state: &Arc<Mutex<AgentRuntimeState>>,
    deps: &AgentRunDeps,
    task: AgentTask,
    cancel: CancellationToken,
) -> Result<AgentTaskResultExt, anyhow::Error> {
    let task_id = task.id.clone().unwrap_or_default();
    let mut transcript = task.history.clone().unwrap_or_default();

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

    run_agent_loop(state, &mut worker, deps, &task_id, cancel).await
}

/// Main model step loop: discover → call model → process outcome → repeat.
///
/// Observability: `#[tracing::instrument]` captures overall loop duration.
/// Per-step detail is in `run_model_step` and `execute_tool_calls`.
#[tracing::instrument(
    skip(state, worker, deps, cancel),
    fields(task_id = %task_id)
)]
pub(crate) async fn run_agent_loop(
    state: &Arc<Mutex<AgentRuntimeState>>,
    worker: &mut AgentWorkerState,
    deps: &AgentRunDeps,
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
        let mut steering_events = Vec::new();
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
                    steering_events.push(Event::TaskSteered {
                        session_id: context.session_id.clone(),
                        task_id: task_id.to_string(),
                        source_task_id: steering.source_task_id,
                        source_agent_id: steering.source_agent_id,
                        message: steering.message,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as i64,
                    });
                }
            }
        }
        emit_events(deps, steering_events);

        worker.step_count += 1;

        // Discover tools
        let (tools, routes) = {
            let s = state.lock().await;
            use crate::adapters::tools::registry::ToolRegistry;
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
        let model_step = match run_model_step(
            state,
            worker,
            deps,
            &model_config,
            &tools,
            task_id,
            cancel.clone(),
        )
        .await
        {
            Ok(output) => output,
            Err(error) => {
                emit_events(deps, error.events);
                return Err(error.error);
            }
        };
        emit_events(deps, model_step.events);
        let assistant_message = model_step.message;

        let assistant_message_id = super::messages::runtime_assistant_message_id(
            task_id,
            &format!("step_{}", worker.step_count),
        );

        // Append assistant message to transcript
        worker.transcript.push(assistant_message.clone());

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
            Some(&assistant_message),
            None,
            &model_settings,
            &routes,
            cancel.clone(),
            &assistant_message_id,
        )
        .await;

        match outcome {
            StepOutcome::Terminal { result } => return Ok(result),
            StepOutcome::Continue { events } => {
                emit_events(deps, events);
                continue;
            }
        }
    }
}

fn emit_events(_deps: &AgentRunDeps, _events: Vec<Event>) {
    // Events are now emitted via stream! macro yield
}
