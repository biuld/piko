use crate::domain::events::event::Event;

use super::helpers::wait_for_next_control_input;
use super::lifecycle::TaskLifecycleUpdate;
use super::step::StepAdvance;
use super::{IterationOutcome, TaskOrchestrator};
use crate::runtime::types::TaskControlMessage;

impl TaskOrchestrator {
    pub(crate) async fn run_iteration(&mut self) -> IterationOutcome {
        let mut events = Vec::new();

        if let Some(cancelled_events) = self.stop_if_cancelled().await {
            return IterationOutcome::Stop(cancelled_events);
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

        if let Some(cancelled_events) = self.stop_if_cancelled().await {
            events.extend(cancelled_events);
            return IterationOutcome::Stop(events);
        }

        let step_cycle = match self.run_step_cycle().await {
            Ok(cycle) => cycle,
            Err(failure) => {
                events.extend(self.handle_step_failure(failure).await);
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
                TaskControlMessage::Steer(msg) => {
                    if self.run_state.is_closed() {
                        continue;
                    }
                    self.run_state.accept_steer(&msg);
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Steered {
                            source_task_id: &msg.source_task_id,
                            source_agent_id: &msg.source_agent_id,
                            message: &msg.message,
                        })
                        .await,
                    );
                }
                TaskControlMessage::Close => {
                    if !self.run_state.is_closed() {
                        self.run_state.close();
                        self.run_state.deactivate_channels();
                        events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Closed).await);
                    }
                }
                TaskControlMessage::Reopen => {
                    if self.run_state.is_closed() {
                        self.run_state.reopen();
                        events.extend(
                            self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                                .await,
                        );
                    }
                }
            }
        }
        events
    }

    async fn wait_for_next_turn(&mut self, summary: String) -> (Vec<Event>, bool) {
        let mut events = Vec::new();
        self.run_state.deactivate_channels();

        match wait_for_next_control_input(&self.ctx, &mut self.run_state.control_rx).await {
            Some(TaskControlMessage::Steer(msg)) => {
                if self.run_state.is_closed() {
                    (events, true)
                } else {
                    self.run_state.accept_steer(&msg);
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Steered {
                            source_task_id: &msg.source_task_id,
                            source_agent_id: &msg.source_agent_id,
                            message: &msg.message,
                        })
                        .await,
                    );
                    events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Started).await);
                    (events, true)
                }
            }
            Some(TaskControlMessage::Close) => {
                if !self.run_state.is_closed() {
                    self.run_state.close();
                    self.run_state.deactivate_channels();
                    events.extend(self.emit_task_lifecycle(TaskLifecycleUpdate::Closed).await);
                }
                (events, true)
            }
            Some(TaskControlMessage::Reopen) => {
                if self.run_state.is_closed() {
                    self.run_state.reopen();
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                            .await,
                    );
                }
                (events, true)
            }
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
            match wait_for_next_control_input(&self.ctx, &mut self.run_state.control_rx).await {
                Some(TaskControlMessage::Close) => {}
                Some(TaskControlMessage::Reopen) => {
                    self.run_state.reopen();
                    events.extend(
                        self.emit_task_lifecycle(TaskLifecycleUpdate::Reopened)
                            .await,
                    );
                    let (next_events, should_continue) =
                        self.wait_for_next_turn(String::new()).await;
                    events.extend(next_events);
                    return (events, should_continue);
                }
                Some(TaskControlMessage::Steer(_)) => {}
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
                let (next_events, should_continue) = self.wait_for_next_turn(summary).await;
                events.extend(next_events);
                if should_continue {
                    IterationOutcome::Continue(events)
                } else {
                    IterationOutcome::Stop(events)
                }
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
                        IterationOutcome::Stop(events)
                    }
                }
            }
        }
    }
}
