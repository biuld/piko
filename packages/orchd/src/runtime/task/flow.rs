use piko_protocol::agent_runtime::TaskControlRequest;

use super::action::{InputAwaitReason, TaskAction};
use super::helpers::wait_for_next_mailbox_message;
use super::input::{commit_mailbox_input, source_task_agent};
use super::lifecycle::TaskLifecycleUpdate;
use super::step::{PendingToolExecution, StepAdvance};
use super::{IterationOutcome, TaskRuntime};
use crate::runtime::task::mailbox::TaskMailboxMessage;

impl TaskRuntime {
    pub(crate) async fn run_iteration(&mut self) -> IterationOutcome {
        loop {
            match self.next_action().await {
                TaskAction::StopCancelled => {
                    self.emit_cancelled().await;
                    return IterationOutcome::Stop;
                }
                TaskAction::StopPersistenceFailure(error) => {
                    tracing::error!(
                        task_id = %self.task_context.task_id(),
                        %error,
                        "stopping task after persistence failure"
                    );
                    return IterationOutcome::Stop;
                }
                TaskAction::ApplyControls => {
                    self.apply_controls().await;
                }
                TaskAction::CommitInput(reason) => {
                    return self.commit_input(reason).await;
                }
                TaskAction::RunStep => {
                    return self.run_step().await;
                }
                TaskAction::ExecuteTools(pending) => {
                    return self.execute_tools(pending).await;
                }
            }
        }
    }

    async fn next_action(&mut self) -> TaskAction {
        if let Some(error) = self.run_state.take_persist_error() {
            return TaskAction::StopPersistenceFailure(error);
        }
        if self.ctx.cancel.is_cancelled() {
            return TaskAction::StopCancelled;
        }

        if !self.run_state.has_user_transcript() {
            return TaskAction::CommitInput(InputAwaitReason::Initial);
        }

        self.run_state.stash_pending_controls();
        if self.run_state.has_stashed_controls() {
            return TaskAction::ApplyControls;
        }

        if self.run_state.is_closed() {
            return TaskAction::CommitInput(InputAwaitReason::WhileClosed);
        }

        if let Some(summary) = self.run_state.take_pending_wait_summary() {
            return TaskAction::CommitInput(InputAwaitReason::NextTurn { summary });
        }

        if let Some(pending) = self.pending_tool_execution.take() {
            return TaskAction::ExecuteTools(pending);
        }

        TaskAction::RunStep
    }

    async fn emit_cancelled(&self) {
        self.emit_task_lifecycle(TaskLifecycleUpdate::Cancelled)
            .await;
    }

    async fn apply_controls(&mut self) {
        for msg in self.run_state.drain_controls() {
            match msg {
                TaskMailboxMessage::Input(mut envelope) => {
                    if self.run_state.is_closed() {
                        envelope.complete_ack(Err("task is closed".into()));
                        continue;
                    }
                    let was_waiting = self.run_state.is_waiting_for_next_turn();
                    if !was_waiting {
                        self.run_state.queue_input(envelope);
                        continue;
                    }
                    let outcome = commit_mailbox_input(
                        &self.task_context,
                        &mut self.run_state,
                        &mut envelope,
                        self.execution.shared_persist_sink(),
                    )
                    .await;
                    if !outcome.committed {
                        continue;
                    }
                    let (source_task_id, source_agent_id) =
                        source_task_agent(&envelope.input.source);
                    if let Some(text) = super::input::input_text(&envelope.input.content) {
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Steered {
                            source_task_id: &source_task_id,
                            source_agent_id: &source_agent_id,
                            message: &text,
                        })
                        .await;
                    }
                    if was_waiting {
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await;
                    }
                }
                TaskMailboxMessage::Control(mut envelope) => match &envelope.request {
                    TaskControlRequest::Close { .. } => {
                        if !self.run_state.is_closed() {
                            self.run_state.close();
                            self.emit_task_lifecycle(TaskLifecycleUpdate::Closed).await;
                        }
                        envelope.complete(Ok(()));
                    }
                    TaskControlRequest::Reopen { .. } => {
                        if self.run_state.is_closed() {
                            self.run_state.reopen();
                            self.run_state.wait_for_next_turn(String::new());
                            self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                                .await;
                            envelope.complete(Ok(()));
                        } else {
                            envelope.complete(Err("task is not closed".into()));
                        }
                    }
                    TaskControlRequest::CancelWork { work_id, .. } => {
                        if self.run_state.active_work_id() == Some(work_id.as_str()) {
                            let emitter = self.run_state.event_emitter_for_active_work(
                                self.task_context.dispatch_identity(),
                            );
                            emitter
                                .emit_work_changed(piko_protocol::agent_runtime::WorkSnapshot {
                                    work_id: work_id.clone(),
                                    status: piko_protocol::agent_runtime::WorkStatus::Cancelled,
                                    source_turn_id: self.current_source_turn_id(),
                                })
                                .await;
                            self.run_state.wait_for_next_turn(String::new());
                            envelope.complete(Ok(()));
                        } else {
                            envelope.complete(Err("work is not active".into()));
                        }
                    }
                    TaskControlRequest::Terminate { .. } => {
                        envelope.complete(Err("terminate is handled by supervisor".into()));
                    }
                },
            }
        }
    }

    async fn commit_input(&mut self, reason: InputAwaitReason) -> IterationOutcome {
        match reason {
            InputAwaitReason::Initial => {
                match wait_for_next_mailbox_message(&self.ctx, &mut self.run_state.control_rx).await
                {
                    Some(TaskMailboxMessage::Input(mut envelope)) => {
                        let outcome = commit_mailbox_input(
                            &self.task_context,
                            &mut self.run_state,
                            &mut envelope,
                            self.execution.shared_persist_sink(),
                        )
                        .await;
                        if outcome.committed {
                            self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await;
                            IterationOutcome::Continue
                        } else {
                            IterationOutcome::Stop
                        }
                    }
                    Some(TaskMailboxMessage::Control(mut envelope)) => {
                        envelope.complete(Err("task has not accepted input".into()));
                        IterationOutcome::Continue
                    }
                    None => {
                        if self.ctx.cancel.is_cancelled() {
                            self.emit_cancelled().await;
                        }
                        IterationOutcome::Stop
                    }
                }
            }
            InputAwaitReason::WhileClosed => {
                if self.wait_while_closed().await {
                    IterationOutcome::Continue
                } else {
                    IterationOutcome::Stop
                }
            }
            InputAwaitReason::NextTurn { summary } => {
                if self.enter_idle(summary).await {
                    IterationOutcome::Continue
                } else {
                    IterationOutcome::Stop
                }
            }
        }
    }

    async fn enter_idle(&mut self, summary: String) -> bool {
        let next_message = self
            .run_state
            .pop_queued_input()
            .map(TaskMailboxMessage::Input);
        let next_message = match next_message {
            Some(message) => Some(message),
            None => wait_for_next_mailbox_message(&self.ctx, &mut self.run_state.control_rx).await,
        };
        match next_message {
            Some(TaskMailboxMessage::Input(mut envelope)) => {
                if self.run_state.is_closed() {
                    true
                } else {
                    let outcome = commit_mailbox_input(
                        &self.task_context,
                        &mut self.run_state,
                        &mut envelope,
                        self.execution.shared_persist_sink(),
                    )
                    .await;
                    if outcome.committed {
                        let (source_task_id, source_agent_id) =
                            source_task_agent(&envelope.input.source);
                        if let Some(text) = super::input::input_text(&envelope.input.content) {
                            self.emit_task_lifecycle(TaskLifecycleUpdate::Steered {
                                source_task_id: &source_task_id,
                                source_agent_id: &source_agent_id,
                                message: &text,
                            })
                            .await;
                        }
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await;
                    }
                    outcome.committed
                }
            }
            Some(TaskMailboxMessage::Control(mut envelope))
                if matches!(envelope.request, TaskControlRequest::Close { .. }) =>
            {
                if !self.run_state.is_closed() {
                    self.run_state.close();
                    self.emit_task_lifecycle(TaskLifecycleUpdate::Closed).await;
                }
                envelope.complete(Ok(()));
                true
            }
            Some(TaskMailboxMessage::Control(mut envelope))
                if matches!(envelope.request, TaskControlRequest::Reopen { .. }) =>
            {
                if self.run_state.is_closed() {
                    self.run_state.reopen();
                    self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                        .await;
                }
                self.run_state.wait_for_next_turn(summary);
                envelope.complete(Ok(()));
                true
            }
            Some(TaskMailboxMessage::Control(mut envelope)) => {
                envelope.complete(Err("control is invalid while task is idle".into()));
                true
            }
            None => {
                if self.ctx.cancel.is_cancelled() {
                    self.emit_cancelled().await;
                } else {
                    self.emit_task_lifecycle(TaskLifecycleUpdate::Completed {
                        total_steps: self.run_state.step_count(),
                        summary: &summary,
                    })
                    .await;
                }
                false
            }
        }
    }

    async fn wait_while_closed(&mut self) -> bool {
        loop {
            match wait_for_next_mailbox_message(&self.ctx, &mut self.run_state.control_rx).await {
                Some(TaskMailboxMessage::Control(mut envelope))
                    if matches!(envelope.request, TaskControlRequest::Close { .. }) =>
                {
                    envelope.complete(Ok(()));
                }
                Some(TaskMailboxMessage::Control(mut envelope))
                    if matches!(envelope.request, TaskControlRequest::Reopen { .. }) =>
                {
                    self.run_state.reopen();
                    self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                        .await;
                    self.run_state.wait_for_next_turn(String::new());
                    envelope.complete(Ok(()));
                    return true;
                }
                Some(TaskMailboxMessage::Input(_)) => {}
                Some(TaskMailboxMessage::Control(mut envelope)) => {
                    envelope.complete(Err("control is invalid while task is closed".into()));
                }
                None => {
                    if self.ctx.cancel.is_cancelled() {
                        self.emit_cancelled().await;
                        return false;
                    }
                }
            }
        }
    }

    async fn run_step(&mut self) -> IterationOutcome {
        if self.ctx.cancel.is_cancelled() {
            self.emit_cancelled().await;
            return IterationOutcome::Stop;
        }

        let step_cycle = match self.run_step_cycle().await {
            Ok(cycle) => cycle,
            Err(failure) => {
                let error = self.handle_step_failure(failure).await;
                if self.run_state.can_follow_up() {
                    self.run_state.wait_for_next_turn(error);
                    return IterationOutcome::Continue;
                }
                return IterationOutcome::Stop;
            }
        };

        if self.ctx.cancel.is_cancelled() {
            self.emit_cancelled().await;
            return IterationOutcome::Stop;
        }

        let advance = self.advance_after_step(step_cycle).await;
        self.resolve_step_advance(advance).await
    }

    async fn execute_tools(&mut self, pending: PendingToolExecution) -> IterationOutcome {
        if self.ctx.cancel.is_cancelled() {
            self.emit_cancelled().await;
            return IterationOutcome::Stop;
        }

        match self
            .execute_tool_calls(&pending.tool_calls, &pending.routes, pending.message_id)
            .await
        {
            Ok(_) => IterationOutcome::Continue,
            Err(error) => {
                self.emit_task_lifecycle(TaskLifecycleUpdate::Failed { error: &error })
                    .await;
                if self.run_state.can_follow_up() {
                    self.run_state.wait_for_next_turn(error);
                    IterationOutcome::Continue
                } else {
                    IterationOutcome::Stop
                }
            }
        }
    }

    async fn resolve_step_advance(&mut self, advance: StepAdvance) -> IterationOutcome {
        match advance {
            StepAdvance::AwaitNextTurn { summary } => {
                self.run_state.wait_for_next_turn(summary);
                IterationOutcome::Continue
            }
            StepAdvance::Stop => IterationOutcome::Stop,
            StepAdvance::ExecuteTools { pending } => {
                self.pending_tool_execution = Some(pending);
                IterationOutcome::Continue
            }
        }
    }
}
