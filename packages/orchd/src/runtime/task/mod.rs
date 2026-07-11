// ---- Task runtime — per-task agent execution loop ----

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::model::step::ModelConfig;
use crate::ports::model_gateway::LlmGateway;
use crate::runtime::persist_sink::SharedPersistSink;

mod action;
mod context;
mod execution;
mod flow;
mod helpers;
pub(crate) mod input;
mod lifecycle;
pub(crate) mod mailbox;
mod state;
mod step;

pub(crate) use mailbox::{TaskControlEnvelope, TaskInputEnvelope, TaskMailboxMessage};

use self::context::TaskContext;
use self::execution::TaskExecution;
use self::helpers::summarize;
use self::lifecycle::{TaskLifecycleEmitter, TaskLifecycleUpdate};
use self::state::TaskRunState;
use self::step::{PendingToolExecution, StepAdvance, StepCycle, StepDispatchFailure};
use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::AgentTask;
use crate::runtime::events::InternalLifecycleObserver;

// ---- Agent run dependencies ----

/// Dependencies injected into an agent run.
#[derive(Clone)]
pub(crate) struct AgentRunDeps {
    pub model_executor: Arc<dyn LlmGateway>,
    pub model_config: Option<ModelConfig>,
    pub tool_registry: Arc<ToolRegistryImpl>,
    pub persist_sink: SharedPersistSink,
    pub output_hub: crate::runtime::events::SharedSessionOutputHub,
    pub lifecycle_observer: InternalLifecycleObserver,
}

// ---- Per-run context ----

pub(crate) struct RunContext {
    #[allow(dead_code)] // held to keep channel alive
    pub control_tx: mpsc::UnboundedSender<TaskMailboxMessage>,
    pub cancel: CancellationToken,
}

pub(crate) enum IterationOutcome {
    Continue,
    Stop,
}

pub(crate) struct TaskRuntime {
    pub(crate) ctx: RunContext,
    task_context: TaskContext,
    run_state: TaskRunState,
    execution: TaskExecution,
    pending_tool_execution: Option<PendingToolExecution>,
}

impl TaskRuntime {
    pub(crate) fn new(
        ctx: RunContext,
        control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
        deps: AgentRunDeps,
        task: AgentTask,
        spec: AgentSpec,
        allow_followup_turns: bool,
    ) -> Self {
        let task_context = TaskContext::new(&task, &spec);
        let output_hub = deps.output_hub.clone();
        let run_state = TaskRunState::new(
            &task,
            control_rx,
            output_hub.clone(),
            deps.persist_sink.clone(),
            deps.lifecycle_observer.clone(),
            allow_followup_turns,
        );
        let execution = TaskExecution::new(deps, spec);

        Self {
            ctx,
            task_context,
            run_state,
            execution,
            pending_tool_execution: None,
        }
    }

    fn current_work_id(&self) -> String {
        self.run_state
            .active_work_id()
            .map(str::to_string)
            .unwrap_or_else(crate::ports::id_generator::generate_work_id)
    }

    fn current_source_turn_id(&self) -> Option<String> {
        self.run_state.active_source_turn_id().map(str::to_string)
    }

    pub(crate) async fn initialize(&mut self) {
        if self.task_context.is_resumed() {
            return;
        }
        let bootstrap_work_id = self.current_work_id();
        self.emit_task_lifecycle_with_work(
            TaskLifecycleUpdate::Created {
                parent_task_id: self.task_context.parent_task_id(),
                source_agent_id: self.task_context.source_agent_id(),
                prompt: self.task_context.prompt(),
                work_id: &bootstrap_work_id,
            },
            &bootstrap_work_id,
        )
        .await;
    }

    async fn run_step_cycle(&mut self) -> Result<StepCycle, StepDispatchFailure> {
        self.execution
            .run_step_cycle(&self.ctx, &self.task_context, &mut self.run_state)
            .await
    }

    async fn execute_tool_calls(
        &mut self,
        tool_calls: &[crate::domain::tools::call::ToolCallItem],
        routes: &std::collections::HashMap<String, crate::adapters::tools::registry::CatalogRoute>,
        message_id: String,
    ) -> Result<crate::runtime::tools::ToolExecutionResult, String> {
        let step_count = self.run_state.step_count();
        self.execution
            .execute_tool_calls(
                &self.task_context,
                &mut self.run_state,
                message_id,
                self.ctx.cancel.clone(),
                step_count,
                tool_calls,
                routes,
            )
            .await
    }

    async fn handle_step_failure(&mut self, failure: StepDispatchFailure) -> String {
        let StepDispatchFailure { error, result: _ } = failure;
        let error = format!("Gateway error: {error}");
        self.emit_task_lifecycle(TaskLifecycleUpdate::Failed { error: &error })
            .await;
        error
    }

    async fn advance_after_step(&mut self, cycle: StepCycle) -> StepAdvance {
        let applied = self.run_state.apply_step_result(cycle);

        if let piko_protocol::Message::Assistant {
            error_message: Some(error),
            ..
        } = &applied.assistant_message
        {
            self.emit_task_lifecycle(TaskLifecycleUpdate::Failed { error })
                .await;
            return if self.run_state.can_follow_up() {
                StepAdvance::AwaitNextTurn {
                    summary: error.clone(),
                }
            } else {
                StepAdvance::Stop
            };
        }

        if applied.tool_calls.is_empty() || !self.execution.allow_tool_calls() {
            let summary = summarize(&applied.assistant_message);
            let session_id = self.task_context.session_id();
            let task_id = self.task_context.task_id();
            let work_id = self.current_work_id();
            if self.run_state.can_follow_up() {
                tracing::info!(
                    session_id = %session_id,
                    task_id = %task_id,
                    work_id = %work_id,
                    step_count = self.run_state.step_count(),
                    "step finished without tools; emitting task idle"
                );
                self.emit_task_lifecycle(TaskLifecycleUpdate::Idle {
                    total_steps: self.run_state.step_count(),
                    summary: &summary,
                })
                .await;
                tracing::info!(
                    session_id = %session_id,
                    task_id = %task_id,
                    work_id = %work_id,
                    "task idle lifecycle emitted; awaiting next turn"
                );
                StepAdvance::AwaitNextTurn { summary }
            } else {
                tracing::info!(
                    session_id = %session_id,
                    task_id = %task_id,
                    work_id = %work_id,
                    step_count = self.run_state.step_count(),
                    "step finished without tools; emitting task completed"
                );
                self.emit_task_lifecycle(TaskLifecycleUpdate::Completed {
                    total_steps: self.run_state.step_count(),
                    summary: &summary,
                })
                .await;
                StepAdvance::Stop
            }
        } else {
            tracing::info!(
                session_id = %self.task_context.session_id(),
                task_id = %self.task_context.task_id(),
                work_id = %self.current_work_id(),
                tool_calls = applied.tool_calls.len(),
                "step finished with tool calls; executing tools"
            );
            StepAdvance::ExecuteTools {
                pending: PendingToolExecution {
                    tool_calls: applied.tool_calls,
                    routes: applied.routes,
                    message_id: applied.message_id,
                },
            }
        }
    }

    async fn emit_task_lifecycle(&self, update: TaskLifecycleUpdate<'_>) {
        self.emit_task_lifecycle_with_work(update, &self.current_work_id())
            .await
    }

    async fn emit_task_lifecycle_with_work(&self, update: TaskLifecycleUpdate<'_>, work_id: &str) {
        let emitter = self
            .run_state
            .event_emitter(self.task_context.dispatch_identity(), work_id.to_string());
        TaskLifecycleEmitter::new(&self.task_context, emitter)
            .emit(update)
            .await
    }
}

pub(crate) async fn run_task(
    ctx: RunContext,
    control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
    deps: AgentRunDeps,
    task: AgentTask,
    spec: AgentSpec,
    allow_followup_turns: bool,
) {
    let mut task_runtime =
        TaskRuntime::new(ctx, control_rx, deps, task, spec, allow_followup_turns);
    task_runtime.initialize().await;

    while let IterationOutcome::Continue = task_runtime.run_iteration().await {}
}
