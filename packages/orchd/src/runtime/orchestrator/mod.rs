// ---- agent_task_stream — async-stream based root agent execution ----

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::events::event::Event;
use crate::domain::model::step::ModelConfig;
use crate::integration::PersistSink;
use crate::ports::model_gateway::LlmGateway;
use crate::runtime::types::TaskMailboxMessage;

use super::dispatch::StepDispatchResult;

mod context;
mod execution;
mod flow;
mod helpers;
pub(crate) mod input;
mod lifecycle;
mod run_state;
mod step;

use self::context::TaskContext;
use self::execution::TaskExecution;
use self::helpers::{summarize, wait_for_next_mailbox_message};
use self::input::{build_user_input, commit_input};
use self::lifecycle::{TaskLifecycleEmitter, TaskLifecycleUpdate};
use self::run_state::TaskRunState;
use self::step::{PendingToolExecution, StepAdvance, StepCycle, StepDispatchFailure};
use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::AgentTask;
use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::InputSource;

// ---- Agent run dependencies ----

/// Dependencies injected into an agent run.
#[derive(Clone)]
pub(crate) struct AgentRunDeps {
    pub model_executor: Arc<dyn LlmGateway>,
    pub model_config: Option<ModelConfig>,
    pub tool_registry: Arc<ToolRegistryImpl>,
    pub persist_sink: Option<Arc<dyn PersistSink>>,
    pub output_hub: Option<crate::runtime::events::SharedSessionOutputHub>,
}

// ---- Per-run context ----

pub(crate) struct RunContext {
    #[allow(dead_code)] // held to keep channel alive
    pub control_tx: mpsc::UnboundedSender<TaskMailboxMessage>,
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
    output_hub: Option<crate::runtime::events::SharedSessionOutputHub>,
}

impl TaskOrchestrator {
    pub(crate) fn new(
        ctx: RunContext,
        control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
        deps: AgentRunDeps,
        task: AgentTask,
        spec: AgentSpec,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
        allow_followup_turns: bool,
    ) -> Self {
        let task_context = TaskContext::new(&task, &spec);
        let output_hub = deps.output_hub.clone();
        let run_state = TaskRunState::new(
            &task,
            control_rx,
            senders,
            output_hub.clone(),
            allow_followup_turns,
        );
        let execution = TaskExecution::new(deps, spec);

        Self {
            ctx,
            task_context,
            run_state,
            execution,
            output_hub,
        }
    }

    pub(crate) async fn initialize_events(&mut self) -> Vec<Event> {
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
        if !self.task_context.prompt().is_empty() {
            let senders = self.run_state.senders_owned();
            let input = build_user_input(
                self.task_context.session_id(),
                self.task_context.task_id(),
                self.task_context.turn_id(),
                MessageContent::String(self.task_context.prompt().to_string()),
                self.task_context
                    .source_agent_id()
                    .map(|agent_id| InputSource::Task {
                        task_id: self
                            .task_context
                            .parent_task_id()
                            .unwrap_or_default()
                            .to_string(),
                        agent_id: agent_id.to_string(),
                    })
                    .unwrap_or(InputSource::User),
            );
            if let Ok(committed) = commit_input(
                &self.task_context,
                &mut self.run_state,
                &input,
                senders,
                self.output_hub.clone(),
                self.execution.persist_sink(),
            )
            .await
            {
                events.extend(committed);
            }
            events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await);
        }
        events
    }

    async fn await_initial_input(&mut self) -> (Vec<Event>, bool) {
        let mut events = Vec::new();
        match wait_for_next_mailbox_message(&self.ctx, &mut self.run_state.control_rx).await {
            Some(TaskMailboxMessage::Input(envelope)) => {
                self.run_state.accept_input(&envelope);
                if let Ok(committed) = commit_input(
                    &self.task_context,
                    &mut self.run_state,
                    &envelope.input,
                    envelope.senders,
                    self.output_hub.clone(),
                    self.execution.persist_sink(),
                )
                .await
                {
                    events.extend(committed);
                }
                events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await);
                (events, true)
            }
            Some(TaskMailboxMessage::Control(_)) => (events, true),
            None => {
                if self.ctx.cancel.is_cancelled() {
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Cancelled)
                            .await,
                    );
                }
                (events, false)
            }
        }
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
                &mut self.run_state,
                message_id,
                self.ctx.cancel.clone(),
                step_count,
                tool_calls,
                routes,
            )
            .await
    }

    async fn handle_step_failure(&mut self, failure: StepDispatchFailure) -> (Vec<Event>, String) {
        let StepDispatchFailure { error, result } = failure;
        let StepDispatchResult { local_output, .. } = result;
        let error = format!("Gateway error: {error}");
        let mut events = self
            .run_state
            .collect_local_step_events(local_output.display, local_output.persist);
        events.extend(
            self.emit_task_lifecycle(TaskLifecycleUpdate::Failed { error: &error })
                .await,
        );
        (events, error)
    }

    async fn advance_after_step(&mut self, cycle: StepCycle) -> StepAdvance {
        let applied = self.run_state.apply_step_result(cycle);

        if let piko_protocol::Message::Assistant {
            error_message: Some(error),
            ..
        } = &applied.assistant_message
        {
            let mut events = applied.events;
            events.extend(
                self.emit_task_lifecycle(TaskLifecycleUpdate::Failed { error })
                    .await,
            );
            return if self.run_state.can_follow_up() {
                StepAdvance::AwaitNextTurn {
                    events,
                    summary: error.clone(),
                }
            } else {
                StepAdvance::Stop { events }
            };
        }

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
        TaskLifecycleEmitter::new(
            &self.task_context,
            self.run_state.senders_owned(),
            self.output_hub.clone(),
            self.run_state.last_task_seq(),
        )
        .emit(update)
        .await
    }
}
