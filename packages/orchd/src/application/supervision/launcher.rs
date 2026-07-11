use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::{AgentTask, HostTaskContext};
use crate::runtime::agent_loop::agent_loop;
use crate::runtime::task::{AgentRunDeps, RunContext};
use piko_protocol::ServerMessage as Event;

use super::supervisor::Supervisor;

pub(crate) async fn spawn_registered_agent_stream(
    supervisor: &Supervisor,
    spec: AgentSpec,
    mut task: AgentTask,
    allow_followup_turns: bool,
) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    if task.host_context.is_none() {
        task.host_context = Some(HostTaskContext::new(supervisor.state.run_id.clone()));
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
        allow_followup_turns,
    ))
}
