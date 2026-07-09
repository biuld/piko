// ---- agent_task_stream — async-stream based root agent execution ----

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::events::event::Event;
use crate::domain::model::step::ModelConfig;
use crate::ports::model_gateway::LlmGateway;
use crate::runtime::types::TaskControlMessage;

use super::dispatch::StepDispatchResult;

mod context;
mod execution;
mod flow;
mod helpers;
mod lifecycle;
mod run_state;
mod step;

use self::context::TaskContext;
use self::execution::TaskExecution;
use self::helpers::summarize;
use self::lifecycle::{TaskLifecycleEmitter, TaskLifecycleUpdate};
use self::run_state::TaskRunState;
use self::step::{PendingToolExecution, StepAdvance, StepCycle, StepDispatchFailure};
use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::AgentTask;
use crate::ports::agent_spawner::AgentSpawner;

// ---- Agent run dependencies ----

/// Dependencies injected into an agent run.
#[derive(Clone)]
pub(crate) struct AgentRunDeps {
    pub model_executor: Arc<dyn LlmGateway>,
    pub model_config: Option<ModelConfig>,
    pub tool_registry: Arc<ToolRegistryImpl>,
}

// ---- Per-run context ----

pub(crate) struct RunContext {
    #[allow(dead_code)] // held to keep channel alive
    pub control_tx: mpsc::UnboundedSender<TaskControlMessage>,
    pub cancel: CancellationToken,
}

pub(crate) enum IterationOutcome {
    Continue(Vec<Event>),
    Stop(Vec<Event>),
}

pub(crate) struct TaskOrchestrator {
    pub(crate) ctx: RunContext,
    task_context: TaskContext,
    run_state: TaskRunState,
    execution: TaskExecution,
}

impl TaskOrchestrator {
    pub(crate) fn new(
        ctx: RunContext,
        control_rx: mpsc::UnboundedReceiver<TaskControlMessage>,
        deps: AgentRunDeps,
        task: AgentTask,
        spec: AgentSpec,
        spawner: Arc<dyn AgentSpawner>,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> Self {
        let task_context = TaskContext::new(&task, &spec);
        let run_state = TaskRunState::new(&task, control_rx, senders);
        let execution = TaskExecution::new(deps, spec, spawner);

        Self {
            ctx,
            task_context,
            run_state,
            execution,
        }
    }

    pub(crate) async fn initialize_events(&self) -> Vec<Event> {
        let mut events = Vec::new();
        events.extend(
            self.emit_task_lifecycle(TaskLifecycleUpdate::Created {
                parent_task_id: self.task_context.parent_task_id(),
                source_agent_id: self.task_context.source_agent_id(),
                prompt: self.task_context.prompt(),
                turn_id: self.task_context.turn_id(),
            })
            .await,
        );
        events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await);
        events
    }

    async fn run_step_cycle(&mut self) -> Result<StepCycle, StepDispatchFailure> {
        self.execution
            .run_step_cycle(&self.ctx, &self.task_context, &mut self.run_state)
            .await
    }

    async fn execute_tool_calls(
        &mut self,
        tool_calls: &[crate::runtime::types::ToolCallItem],
        routes: &std::collections::HashMap<String, crate::adapters::tools::registry::CatalogRoute>,
        message_id: String,
    ) -> Result<crate::runtime::tool_executor::ToolExecutionResult, String> {
        let step_count = self.run_state.step_count();
        self.execution
            .execute_tool_calls(
                &self.task_context,
                self.run_state.senders_owned(),
                message_id,
                self.ctx.cancel.clone(),
                self.run_state.transcript_mut(),
                step_count,
                tool_calls,
                routes,
            )
            .await
    }

    async fn handle_step_failure(&mut self, failure: StepDispatchFailure) -> Vec<Event> {
        let StepDispatchFailure { error, result } = failure;
        let StepDispatchResult { local_output, .. } = result;
        let mut events = self
            .run_state
            .collect_local_step_events(local_output.display, local_output.persist);
        events.extend(
            self.emit_task_lifecycle(TaskLifecycleUpdate::Failed {
                error: &format!("Gateway error: {error}"),
            })
            .await,
        );
        events
    }

    async fn advance_after_step(&mut self, cycle: StepCycle) -> StepAdvance {
        let applied = self.run_state.apply_step_result(cycle);

        if applied.tool_calls.is_empty() || !self.execution.allow_tool_calls() {
            let summary = summarize(&applied.assistant_message);
            let mut events = applied.events;
            if self.run_state.can_follow_up() {
                events.extend(
                    self.emit_task_lifecycle(TaskLifecycleUpdate::Idle {
                        total_steps: self.run_state.step_count(),
                        summary: &summary,
                    })
                    .await,
                );
                StepAdvance::AwaitNextTurn { events, summary }
            } else {
                events.extend(
                    self.emit_task_lifecycle(TaskLifecycleUpdate::Completed {
                        total_steps: self.run_state.step_count(),
                        summary: &summary,
                    })
                    .await,
                );
                StepAdvance::Stop { events }
            }
        } else {
            StepAdvance::ExecuteTools {
                events: applied.events,
                pending: PendingToolExecution {
                    tool_calls: applied.tool_calls,
                    routes: applied.routes,
                    message_id: applied.message_id,
                },
            }
        }
    }

    async fn emit_task_lifecycle(&self, update: TaskLifecycleUpdate<'_>) -> Vec<Event> {
        TaskLifecycleEmitter::new(&self.task_context, self.run_state.senders_owned())
            .emit(update)
            .await
    }
}
