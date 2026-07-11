use async_stream::stream;
use futures_core::Stream;
use tokio::sync::mpsc;

use crate::domain::Event;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::AgentTask;
use crate::runtime::task::mailbox::TaskMailboxMessage;

use super::{AgentRunDeps, IterationOutcome, RunContext, TaskRuntime};

#[allow(unused_assignments)]
pub(crate) fn orchestrator(
    ctx: RunContext,
    control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
    deps: AgentRunDeps,
    task: AgentTask,
    spec: AgentSpec,
    allow_followup_turns: bool,
) -> impl Stream<Item = Event> {
    stream! {
        let mut task_runtime = TaskRuntime::new(
            ctx,
            control_rx,
            deps,
            task,
            spec,
            allow_followup_turns,
        );
        for event in task_runtime.initialize_events().await {
            yield event;
        }

        loop {
            match task_runtime.run_iteration().await {
                IterationOutcome::Continue(events) => {
                    for event in events {
                        yield event;
                    }
                }
                IterationOutcome::Stop(events) => {
                    for event in events {
                        yield event;
                    }
                    break;
                }
            }
        }
    }
}
