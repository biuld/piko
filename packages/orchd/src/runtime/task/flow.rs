use piko_protocol::agent_runtime::TaskControlRequest;

use crate::domain::events::event::Event;

use super::helpers::wait_for_next_mailbox_message;
use super::input::{commit_input, source_task_agent};
use super::lifecycle::TaskLifecycleUpdate;
use super::step::StepAdvance;
use super::{IterationOutcome, TaskRuntime};
use crate::runtime::types::TaskMailboxMessage;

impl TaskRuntime {
    pub(crate) async fn run_iteration(&mut self) -> IterationOutcome {
        let mut events = Vec::new();

        if let Some(cancelled_events) = self.stop_if_cancelled().await {
            return IterationOutcome::Stop(cancelled_events);
        }

        if !self.run_state.has_user_transcript() {
            let (input_events, should_continue) = self.await_initial_input().await;
            events.extend(input_events);
            if !should_continue {
                return IterationOutcome::Stop(events);
            }
        }

        events.extend(self.drain_pending_controls().await);

        if self.run_state.is_closed() {
            let (next_events, should_continue) = self.wait_while_closed().await;
            events.extend(next_events);
            if should_continue {
                return IterationOutcome::Continue(events);
            }
            return IterationOutcome::Stop(events);
        }

        if let Some(summary) = self.run_state.take_pending_wait_summary() {
            let (next_events, should_continue) = self.wait_for_next_turn(summary).await;
            events.extend(next_events);
            if should_continue {
                return IterationOutcome::Continue(events);
            }
            return IterationOutcome::Stop(events);
        }

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

    async fn cancelled_event(&self) -> Vec<Event> {
        self.emit_task_lifecycle(TaskLifecycleUpdate::Cancelled)
            .await
    }

    async fn stop_if_cancelled(&self) -> Option<Vec<Event>> {
        if self.ctx.cancel.is_cancelled() {
            Some(self.cancelled_event().await)
        } else {
            None
        }
    }

    async fn drain_pending_controls(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for msg in self.run_state.drain_controls() {
            match msg {
                TaskMailboxMessage::Input(envelope) => {
                    if self.run_state.is_closed() {
                        continue;
                    }
                    let was_waiting = self.run_state.is_waiting_for_next_turn();
                    self.run_state.accept_input(&envelope);
                    let (source_task_id, source_agent_id) =
                        source_task_agent(&envelope.input.source);
                    events.extend(
                        match commit_input(
                            &self.task_context,
                            &mut self.run_state,
                            &envelope.input,
                            self.execution.persist_sink(),
                        )
                        .await
                        {
                            Ok(events) => events,
                            Err(error) => {
                                tracing::error!(%error, "failed to commit task input");
                                Vec::new()
                            }
                        },
                    );
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
                TaskMailboxMessage::Control(TaskControlRequest::Close { .. }) => {
                    if !self.run_state.is_closed() {
                        self.run_state.close();
                        events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Closed).await);
                    }
                }
                TaskMailboxMessage::Control(TaskControlRequest::Reopen { .. }) => {
                    if self.run_state.is_closed() {
                        self.run_state.reopen();
                        self.run_state.wait_for_next_turn(String::new());
                        events.extend(
                            self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                                .await,
                        );
                    }
                }
                TaskMailboxMessage::Control(_) => {}
            }
        }
        events
    }

    async fn wait_for_next_turn(&mut self, summary: String) -> (Vec<Event>, bool) {
        let mut events = Vec::new();

        match wait_for_next_mailbox_message(&self.ctx, &mut self.run_state.control_rx).await {
            Some(TaskMailboxMessage::Input(envelope)) => {
                if self.run_state.is_closed() {
                    (events, true)
                } else {
                    self.run_state.accept_input(&envelope);
                    let (source_task_id, source_agent_id) =
                        source_task_agent(&envelope.input.source);
                    events.extend(
                        match commit_input(
                            &self.task_context,
                            &mut self.run_state,
                            &envelope.input,
                            self.execution.persist_sink(),
                        )
                        .await
                        {
                            Ok(events) => events,
                            Err(error) => {
                                tracing::error!(%error, "failed to commit task input");
                                Vec::new()
                            }
                        },
                    );
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
                    (events, true)
                }
            }
            Some(TaskMailboxMessage::Control(TaskControlRequest::Close { .. })) => {
                if !self.run_state.is_closed() {
                    self.run_state.close();
                    events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Closed).await);
                }
                (events, true)
            }
            Some(TaskMailboxMessage::Control(TaskControlRequest::Reopen { .. })) => {
                if self.run_state.is_closed() {
                    self.run_state.reopen();
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                            .await,
                    );
                }
                self.run_state.wait_for_next_turn(summary);
                (events, true)
            }
            Some(TaskMailboxMessage::Control(_)) => (events, true),
            None => {
                if self.ctx.cancel.is_cancelled() {
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Cancelled)
                            .await,
                    );
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
                Some(TaskMailboxMessage::Control(TaskControlRequest::Close { .. })) => {}
                Some(TaskMailboxMessage::Control(TaskControlRequest::Reopen { .. })) => {
                    self.run_state.reopen();
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                            .await,
                    );
                    self.run_state.wait_for_next_turn(String::new());
                    return (events, true);
                }
                Some(TaskMailboxMessage::Input(_)) => {}
                Some(TaskMailboxMessage::Control(_)) => {}
                None => {
                    if self.ctx.cancel.is_cancelled() {
                        events.extend(
                            self.emit_task_lifecycle(TaskLifecycleUpdate::Cancelled)
                                .await,
                        );
                        return (events, false);
                    }
                }
            }
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
        }
    }
}
