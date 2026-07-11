use piko_protocol::agent_runtime::TaskControlRequest;

use crate::domain::events::event::Event;

use super::action::{InputAwaitReason, TaskAction};
use super::helpers::wait_for_next_mailbox_message;
use super::input::{commit_mailbox_input, source_task_agent};
use super::lifecycle::TaskLifecycleUpdate;
use super::step::{PendingToolExecution, StepAdvance};
use super::{IterationOutcome, TaskRuntime};
use crate::runtime::types::TaskMailboxMessage;

impl TaskRuntime {
    pub(crate) async fn run_iteration(&mut self) -> IterationOutcome {
        let mut events = Vec::new();

        loop {
            match self.next_action().await {
                TaskAction::StopCancelled => {
                    events.extend(self.cancelled_event().await);
                    return IterationOutcome::Stop(events);
                }
                TaskAction::StopPersistenceFailure(error) => {
                    tracing::error!(task_id = %self.task_context.task_id(), %error, "stopping task after persistence failure");
                    return IterationOutcome::Stop(events);
                }
                TaskAction::ApplyControls => {
                    events.extend(self.apply_controls().await);
                }
                TaskAction::CommitInput(reason) => {
                    return self.commit_input(reason, events).await;
                }
                TaskAction::RunStep => {
                    return self.run_step(events).await;
                }
                TaskAction::ExecuteTools(pending) => {
                    return self.execute_tools(events, pending).await;
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

    async fn cancelled_event(&self) -> Vec<Event> {
        self.emit_task_lifecycle(TaskLifecycleUpdate::Cancelled)
            .await
    }

    async fn apply_controls(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
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
                        self.execution.persist_sink(),
                    )
                    .await;
                    events.extend(outcome.events);
                    if !outcome.committed {
                        continue;
                    }
                    let (source_task_id, source_agent_id) =
                        source_task_agent(&envelope.input.source);
                    if let Some(text) = super::input::input_text(&envelope.input.content) {
                        events.extend(
                            self.emit_task_lifecycle(TaskLifecycleUpdate::Steered {
                                source_task_id: &source_task_id,
                                source_agent_id: &source_agent_id,
                                message: &text,
                            })
                            .await,
                        );
                    }
                    if was_waiting {
                        events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await);
                    }
                }
                TaskMailboxMessage::Control(mut envelope) => match &envelope.request {
                    TaskControlRequest::Close { .. } => {
                        if !self.run_state.is_closed() {
                            self.run_state.close();
                            events.extend(
                                self.emit_task_lifecycle(TaskLifecycleUpdate::Closed).await,
                            );
                        }
                        envelope.complete(Ok(()));
                    }
                    TaskControlRequest::Reopen { .. } => {
                        if self.run_state.is_closed() {
                            self.run_state.reopen();
                            self.run_state.wait_for_next_turn(String::new());
                            events.extend(
                                self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                                    .await,
                            );
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
        events
    }

    async fn commit_input(
        &mut self,
        reason: InputAwaitReason,
        mut events: Vec<Event>,
    ) -> IterationOutcome {
        match reason {
            InputAwaitReason::Initial => {
                match wait_for_next_mailbox_message(&self.ctx, &mut self.run_state.control_rx).await
                {
                    Some(TaskMailboxMessage::Input(mut envelope)) => {
                        let outcome = commit_mailbox_input(
                            &self.task_context,
                            &mut self.run_state,
                            &mut envelope,
                            self.execution.persist_sink(),
                        )
                        .await;
                        events.extend(outcome.events);
                        if outcome.committed {
                            events.extend(
                                self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await,
                            );
                            IterationOutcome::Continue(events)
                        } else {
                            IterationOutcome::Stop(events)
                        }
                    }
                    Some(TaskMailboxMessage::Control(mut envelope)) => {
                        envelope.complete(Err("task has not accepted input".into()));
                        IterationOutcome::Continue(events)
                    }
                    None => {
                        if self.ctx.cancel.is_cancelled() {
                            events.extend(self.cancelled_event().await);
                        }
                        IterationOutcome::Stop(events)
                    }
                }
            }
            InputAwaitReason::WhileClosed => {
                let (next_events, should_continue) = self.wait_while_closed().await;
                events.extend(next_events);
                if should_continue {
                    IterationOutcome::Continue(events)
                } else {
                    IterationOutcome::Stop(events)
                }
            }
            InputAwaitReason::NextTurn { summary } => {
                let (next_events, should_continue) = self.enter_idle(summary).await;
                events.extend(next_events);
                if should_continue {
                    IterationOutcome::Continue(events)
                } else {
                    IterationOutcome::Stop(events)
                }
            }
        }
    }

    async fn enter_idle(&mut self, summary: String) -> (Vec<Event>, bool) {
        let mut events = Vec::new();

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
                    (events, true)
                } else {
                    let outcome = commit_mailbox_input(
                        &self.task_context,
                        &mut self.run_state,
                        &mut envelope,
                        self.execution.persist_sink(),
                    )
                    .await;
                    events.extend(outcome.events);
                    if outcome.committed {
                        let (source_task_id, source_agent_id) =
                            source_task_agent(&envelope.input.source);
                        if let Some(text) = super::input::input_text(&envelope.input.content) {
                            events.extend(
                                self.emit_task_lifecycle(TaskLifecycleUpdate::Steered {
                                    source_task_id: &source_task_id,
                                    source_agent_id: &source_agent_id,
                                    message: &text,
                                })
                                .await,
                            );
                        }
                        events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await);
                    }
                    (events, outcome.committed)
                }
            }
            Some(TaskMailboxMessage::Control(mut envelope))
                if matches!(envelope.request, TaskControlRequest::Close { .. }) =>
            {
                if !self.run_state.is_closed() {
                    self.run_state.close();
                    events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Closed).await);
                }
                envelope.complete(Ok(()));
                (events, true)
            }
            Some(TaskMailboxMessage::Control(mut envelope))
                if matches!(envelope.request, TaskControlRequest::Reopen { .. }) =>
            {
                if self.run_state.is_closed() {
                    self.run_state.reopen();
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                            .await,
                    );
                }
                self.run_state.wait_for_next_turn(summary);
                envelope.complete(Ok(()));
                (events, true)
            }
            Some(TaskMailboxMessage::Control(mut envelope)) => {
                envelope.complete(Err("control is invalid while task is idle".into()));
                (events, true)
            }
            None => {
                if self.ctx.cancel.is_cancelled() {
                    events.extend(self.cancelled_event().await);
                } else {
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Completed {
                            total_steps: self.run_state.step_count(),
                            summary: &summary,
                        })
                        .await,
                    );
                }
                (events, false)
            }
        }
    }

    async fn wait_while_closed(&mut self) -> (Vec<Event>, bool) {
        let mut events = Vec::new();
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
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                            .await,
                    );
                    self.run_state.wait_for_next_turn(String::new());
                    envelope.complete(Ok(()));
                    return (events, true);
                }
                Some(TaskMailboxMessage::Input(_)) => {}
                Some(TaskMailboxMessage::Control(mut envelope)) => {
                    envelope.complete(Err("control is invalid while task is closed".into()));
                }
                None => {
                    if self.ctx.cancel.is_cancelled() {
                        events.extend(self.cancelled_event().await);
                        return (events, false);
                    }
                }
            }
        }
    }

    async fn run_step(&mut self, mut events: Vec<Event>) -> IterationOutcome {
        if let Some(cancelled_events) = self.stop_if_cancelled().await {
            events.extend(cancelled_events);
            return IterationOutcome::Stop(events);
        }

        let step_cycle = match self.run_step_cycle().await {
            Ok(cycle) => cycle,
            Err(failure) => {
                let (failure_events, summary) = self.handle_step_failure(failure).await;
                events.extend(failure_events);
                if self.run_state.can_follow_up() {
                    self.run_state.wait_for_next_turn(summary);
                    return IterationOutcome::Continue(events);
                }
                return IterationOutcome::Stop(events);
            }
        };

        if let Some(cancelled_events) = self.stop_if_cancelled().await {
            events.extend(cancelled_events);
            return IterationOutcome::Stop(events);
        }

        let advance = self.advance_after_step(step_cycle).await;
        self.resolve_step_advance(events, advance).await
    }

    async fn execute_tools(
        &mut self,
        mut events: Vec<Event>,
        pending: PendingToolExecution,
    ) -> IterationOutcome {
        if let Some(cancelled_events) = self.stop_if_cancelled().await {
            events.extend(cancelled_events);
            return IterationOutcome::Stop(events);
        }

        match self
            .execute_tool_calls(&pending.tool_calls, &pending.routes, pending.message_id)
            .await
        {
            Ok(result) => {
                events.extend(result.events);
                IterationOutcome::Continue(events)
            }
            Err(error) => {
                events.extend(
                    self.emit_task_lifecycle(TaskLifecycleUpdate::Failed { error: &error })
                        .await,
                );
                if self.run_state.can_follow_up() {
                    self.run_state.wait_for_next_turn(error);
                    IterationOutcome::Continue(events)
                } else {
                    IterationOutcome::Stop(events)
                }
            }
        }
    }

    async fn stop_if_cancelled(&self) -> Option<Vec<Event>> {
        if self.ctx.cancel.is_cancelled() {
            Some(self.cancelled_event().await)
        } else {
            None
        }
    }

    async fn resolve_step_advance(
        &mut self,
        mut events: Vec<Event>,
        advance: StepAdvance,
    ) -> IterationOutcome {
        match advance {
            StepAdvance::AwaitNextTurn {
                events: step_events,
                summary,
            } => {
                events.extend(step_events);
                self.run_state.wait_for_next_turn(summary);
                IterationOutcome::Continue(events)
            }
            StepAdvance::Stop {
                events: step_events,
            } => {
                events.extend(step_events);
                IterationOutcome::Stop(events)
            }
            StepAdvance::ExecuteTools {
                events: step_events,
                pending,
            } => {
                events.extend(step_events);
                self.pending_tool_execution = Some(pending);
                IterationOutcome::Continue(events)
            }
        }
    }
}
