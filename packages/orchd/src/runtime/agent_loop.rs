use std::sync::Arc;

use async_stream::stream;
use futures_core::Stream;
use tokio::sync::mpsc;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::events::event::Event;
use crate::domain::tasks::task::AgentTask;
use crate::ports::agent_spawner::AgentSpawner;
use crate::runtime::types::SteerMessage;

use super::orchestrator::{AgentRunDeps, RunContext, StepAdvance, TaskOrchestrator};

#[allow(unused_assignments)]
pub(crate) fn agent_loop(
    ctx: RunContext,
    steer_rx: mpsc::UnboundedReceiver<SteerMessage>,
    deps: AgentRunDeps,
    task: AgentTask,
    spec: AgentSpec,
    spawner: Arc<dyn AgentSpawner>,
    senders: Option<crate::runtime::dispatch::DispatchSenders>,
) -> impl Stream<Item = Event> {
    stream! {
        let mut orchestrator = TaskOrchestrator::new(
            ctx,
            steer_rx,
            deps,
            task,
            spec,
            spawner,
            senders,
        );
        for event in orchestrator.initialize_events().await {
            yield event;
        }

        'agent: loop {
            // ── Cancel check (top of loop) ──
            if orchestrator.ctx.cancel.is_cancelled() {
                if let Some(ev) = orchestrator.cancelled_event().await { yield ev; }
                break 'agent;
            }

            // ── Drain steering messages ──
            for event in orchestrator.drain_pending_steers().await {
                yield event;
            }

            // ── Cancel check (after discover) ──
            if orchestrator.ctx.cancel.is_cancelled() {
                if let Some(ev) = orchestrator.cancelled_event().await { yield ev; }
                break 'agent;
            }

            let step_cycle = match orchestrator.run_step_cycle().await {
                Ok(cycle) => cycle,
                Err(failure) => {
                    for event in orchestrator.handle_step_failure(failure).await {
                        yield event;
                    }
                    break 'agent;
                }
            };

            // ── Cancel check (after tool calls built) ──
            if orchestrator.ctx.cancel.is_cancelled() {
                if let Some(ev) = orchestrator.cancelled_event().await { yield ev; }
                break 'agent;
            }

            match orchestrator.advance_after_step(step_cycle).await {
                StepAdvance::AwaitNextTurn { events, summary } => {
                    for event in events {
                        yield event;
                    }
                    let (next_events, should_continue) = orchestrator.wait_for_next_turn(summary).await;
                    for event in next_events {
                        yield event;
                    }
                    if should_continue {
                        continue 'agent;
                    }
                    break 'agent;
                }
                StepAdvance::ExecuteTools { events, pending } => {
                    for event in events {
                        yield event;
                    }
                    match orchestrator
                        .execute_tool_calls(
                            &pending.tool_calls,
                            &pending.routes,
                            pending.message_id,
                        )
                        .await
                    {
                        Ok(result) => {
                            let super::tool_executor::ToolExecutionResult { events, .. } = result;
                            for ev in events {
                                yield ev;
                            }
                        }
                        Err(error) => {
                            if let Some(ev) = orchestrator.lifecycle_dispatcher.failed(error).await {
                                yield ev;
                            }
                            break 'agent;
                        }
                    }
                }
            }
        }
    }
}
