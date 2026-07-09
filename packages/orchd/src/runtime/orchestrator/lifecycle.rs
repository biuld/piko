use crate::domain::events::event::Event;
use crate::runtime::dispatch::DispatchSenders;

use super::context::TaskContext;

pub(super) enum TaskLifecycleUpdate<'a> {
    Created {
        parent_task_id: Option<&'a str>,
        source_agent_id: Option<&'a str>,
        prompt: &'a str,
        turn_id: &'a str,
    },
    Started,
    Steered {
        source_task_id: &'a str,
        source_agent_id: &'a str,
        message: &'a str,
    },
    Idle {
        total_steps: u32,
        summary: &'a str,
    },
    Failed {
        error: &'a str,
    },
    Completed {
        total_steps: u32,
        summary: &'a str,
    },
    Cancelled,
    Closed,
    Reopened,
}

pub(super) struct TaskLifecycleEmitter<'a> {
    task_context: &'a TaskContext,
    senders: Option<DispatchSenders>,
}

impl<'a> TaskLifecycleEmitter<'a> {
    pub(super) fn new(task_context: &'a TaskContext, senders: Option<DispatchSenders>) -> Self {
        Self {
            task_context,
            senders,
        }
    }

    pub(super) async fn emit(&self, update: TaskLifecycleUpdate<'_>) -> Vec<Event> {
        let consumer = self.task_context.lifecycle_consumer(self.senders.clone());

        match update {
            TaskLifecycleUpdate::Created {
                parent_task_id,
                source_agent_id,
                prompt,
                turn_id,
            } => {
                consumer
                    .on_task_created(parent_task_id, source_agent_id, prompt, turn_id)
                    .await;
            }
            TaskLifecycleUpdate::Started => {
                consumer.on_task_started().await;
            }
            TaskLifecycleUpdate::Steered {
                source_task_id,
                source_agent_id,
                message,
            } => {
                consumer
                    .on_task_steered(source_task_id, source_agent_id, message)
                    .await;
            }
            TaskLifecycleUpdate::Idle {
                total_steps,
                summary,
            } => {
                consumer.on_task_idle(total_steps, summary).await;
            }
            TaskLifecycleUpdate::Failed { error } => {
                consumer.on_task_failed(error).await;
            }
            TaskLifecycleUpdate::Completed {
                total_steps,
                summary,
            } => {
                consumer.on_task_completed(total_steps, summary).await;
            }
            TaskLifecycleUpdate::Cancelled => {
                consumer.on_task_cancelled().await;
            }
            TaskLifecycleUpdate::Closed => {
                consumer.on_task_closed().await;
            }
            TaskLifecycleUpdate::Reopened => {
                consumer.on_task_reopened().await;
            }
        }

        consumer.take_events()
    }
}
