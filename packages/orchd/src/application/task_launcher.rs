use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::{AgentTask, HostTaskContext};
use crate::ports::agent_spawner::AgentSpawner;
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
) -> Option<SessionChannels> {
    let task_id = state
        .registry
        .active_root_task_for_agent(target_agent)
        .await?;
    let handle = state.registry.handle(&task_id).await?;

    let mut channels = SessionChannels::new(ChannelConfig::default());
    channels.spawn_lifecycle_dispatch(state.run_id.clone());
    let senders = channels.senders();

    let _ = handle
        .control_tx
        .send(TaskControlMessage::Steer(TaskSteerMessage {
            source_task_id: String::new(),
            source_agent_id: String::new(),
            message: prompt.to_string(),
            senders: Some(senders),
        }));

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
    let (task_event_tx, mut task_event_rx) = mpsc::unbounded_channel();
    channels.spawn_lifecycle_dispatch_with_observer(session_id, task_event_tx);

    tokio::spawn(async move {
        while let Some(task_event) = task_event_rx.recv().await {
            state.registry.apply_task_event(&task_event).await;
        }
    });

    channels
}

pub(crate) async fn spawn_registered_agent_stream(
    supervisor: &Supervisor,
    spec: AgentSpec,
    task: AgentTask,
    senders: Option<DispatchSenders>,
) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
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
    let spawner: Arc<dyn AgentSpawner> = Arc::new(Supervisor {
        state: Arc::clone(&supervisor.state),
    });

    Box::pin(agent_loop(
        ctx, control_rx, deps, task, spec, spawner, senders,
    ))
}
