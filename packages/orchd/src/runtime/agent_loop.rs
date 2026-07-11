use async_stream::stream;
use futures_core::Stream;
use tokio::sync::mpsc;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::events::event::Event;
use crate::domain::tasks::task::AgentTask;
use crate::runtime::types::TaskMailboxMessage;

use super::orchestrator::{AgentRunDeps, IterationOutcome, RunContext, TaskOrchestrator};

#[allow(unused_assignments)]
pub(crate) fn agent_loop(
    ctx: RunContext,
    control_rx: mpsc::UnboundedReceiver<TaskMailboxMessage>,
    deps: AgentRunDeps,
    task: AgentTask,
    spec: AgentSpec,
    allow_followup_turns: bool,
) -> impl Stream<Item = Event> {
    stream! {
        let mut orchestrator = TaskOrchestrator::new(
            ctx,
            control_rx,
            deps,
            task,
            spec,
            allow_followup_turns,
        );
        for event in orchestrator.initialize_events().await {
            yield event;
        }

        loop {
            match orchestrator.run_iteration().await {
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
