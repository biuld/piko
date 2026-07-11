use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::{AgentTask, HostTaskContext};
use crate::runtime::task::run_task;
use crate::runtime::task::{AgentRunDeps, RunContext};

use super::driver::spawn_task_runtime;
use super::supervisor::Supervisor;

pub(crate) async fn spawn_registered_agent_task(
    supervisor: &Supervisor,
    spec: AgentSpec,
    mut task: AgentTask,
    allow_followup_turns: bool,
) {
    if task.host_context.is_none() {
        task.host_context = Some(HostTaskContext::new(supervisor.state.run_id.clone()));
    }
    let task_id = task.id.clone().unwrap_or_default();
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
    let output_hub = supervisor.session_hub(&session_id).await;

    let deps = AgentRunDeps {
        model_executor: Arc::clone(&supervisor.state.model_executor),
        model_config: supervisor.state.model_config.read().await.clone(),
        tool_registry: Arc::clone(&supervisor.state.tool_registry),
        persist_sink: supervisor
            .persist_sink()
            .await
            .expect("task runtime must be created through persistence-checked AgentRuntime API"),
        output_hub,
        lifecycle_observer: supervisor.state.lifecycle_observer.clone(),
    };

    let ctx = RunContext { control_tx, cancel };

    spawn_task_runtime(
        Arc::clone(&supervisor.state),
        task_id,
        run_task(ctx, control_rx, deps, task, spec, allow_followup_turns),
    );
}
