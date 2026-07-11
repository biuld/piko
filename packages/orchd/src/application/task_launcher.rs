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
use piko_protocol::ServerMessage as Event;

use super::supervisor::{Supervisor, SupervisorState};

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

    let session_id = task
        .host_context
        .as_ref()
        .map(|hc| hc.session_id.clone())
        .unwrap_or_else(|| supervisor.state.run_id.clone());
    let output_hub = Some(supervisor.session_hub(&session_id).await);

    let deps = AgentRunDeps {
        model_executor: Arc::clone(&supervisor.state.model_executor),
        model_config: supervisor.state.model_config.read().await.clone(),
        tool_registry: Arc::clone(&supervisor.state.tool_registry),
        persist_sink: supervisor.persist_sink().await,
        output_hub,
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
