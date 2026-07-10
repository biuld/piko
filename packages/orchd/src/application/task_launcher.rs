use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::{AgentTask, HostTaskContext};
use crate::runtime::agent_loop::agent_loop;
use crate::runtime::dispatch::{ChannelConfig, DispatchSenders, SessionChannels};
use crate::runtime::orchestrator::{AgentRunDeps, RunContext};
use crate::runtime::types::{TaskControlMessage, TaskSteerMessage};
use piko_protocol::ServerMessage as Event;

use super::supervisor::{Supervisor, SupervisorState};

pub(crate) async fn try_reuse_root_task(
    state: Arc<SupervisorState>,
    target_agent: &str,
    prompt: &str,
    session_id: &str,
) -> Option<SessionChannels> {
    let task_id = state
        .registry
        .active_root_task_for_agent(target_agent, session_id)
        .await?;
    let handle = state.registry.handle(&task_id).await?;

    let channel_context = HostTaskContext {
        session_id: session_id.to_string(),
        turn_id: String::new(),
    };
    let channels = root_session_channels(Arc::clone(&state), Some(&channel_context));
    let senders = channels.senders();

    if handle
        .control_tx
        .send(TaskControlMessage::Steer(TaskSteerMessage {
            source_task_id: String::new(),
            source_agent_id: String::new(),
            message: prompt.to_string(),
            senders: Some(senders),
        }))
        .is_err()
    {
        state.registry.cleanup_runtime(&task_id).await;
        return None;
    }

    Some(channels)
}

pub(crate) fn root_session_channels(
    state: Arc<SupervisorState>,
    host_context: Option<&HostTaskContext>,
) -> SessionChannels {
    let mut channels = SessionChannels::new(ChannelConfig::default());
    let session_id = host_context
        .map(|ctx| ctx.session_id.clone())
        .unwrap_or_else(|| state.run_id.clone());
    channels.spawn_lifecycle_dispatch_with_observer(session_id, state.task_event_tx.clone());

    channels
}

pub(crate) async fn spawn_registered_agent_stream(
    supervisor: &Supervisor,
    spec: AgentSpec,
    mut task: AgentTask,
    senders: Option<DispatchSenders>,
    allow_followup_turns: bool,
) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    supervisor.state.ensure_task_event_projector();
    if task.host_context.is_none() {
        task.host_context = Some(HostTaskContext {
            session_id: supervisor.state.run_id.clone(),
            turn_id: task.id.clone().expect("task id missing"),
        });
    }
    let (control_tx, control_rx) = mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    supervisor
        .register_task_runtime(
            &task,
            &task.target_agent_id,
            cancel.clone(),
            control_tx.clone(),
        )
        .await;

    let deps = AgentRunDeps {
        model_executor: Arc::clone(&supervisor.state.model_executor),
        model_config: supervisor.state.model_config.read().await.clone(),
        tool_registry: Arc::clone(&supervisor.state.tool_registry),
    };

    let ctx = RunContext { control_tx, cancel };

    Box::pin(agent_loop(
        ctx,
        control_rx,
        deps,
        task,
        spec,
        senders,
        allow_followup_turns,
    ))
}
