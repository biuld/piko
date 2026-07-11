use piko_protocol::agent_runtime::{
    SessionCursor, SessionRuntimeSnapshot, SubscribeRequest, TaskSnapshot,
};

use crate::api::{AgentApiError, SessionSubscription};

use super::super::supervision::Supervisor;
use super::task_snapshot::map_task_status;

pub(crate) async fn session_snapshot(
    supervisor: &Supervisor,
    session_id: String,
) -> Result<SessionRuntimeSnapshot, AgentApiError> {
    let tasks = supervisor.state.registry.tasks_snapshot().await;
    let dag = supervisor.state.registry.task_dag_snapshot().await;
    let mut snapshots = Vec::new();
    let mut root_task_id = None;

    for (task_id, task) in &tasks {
        if dag
            .get(task_id)
            .and_then(|parent| parent.as_ref())
            .is_none()
        {
            root_task_id.get_or_insert_with(|| task_id.clone());
        }
        snapshots.push(TaskSnapshot {
            session_id: session_id.clone(),
            task_id: task.id.clone(),
            agent_id: task.target_agent_id.clone(),
            parent_task_id: task.parent_task_id.clone(),
            status: map_task_status(&task.status),
            active_work: supervisor
                .state
                .registry
                .active_work_snapshot(task_id)
                .await,
        });
    }

    Ok(SessionRuntimeSnapshot {
        session_id,
        root_task_id: root_task_id.clone(),
        active_task_id: root_task_id,
        tasks: snapshots,
        cursor: SessionCursor {
            epoch: supervisor.state.run_id.clone(),
            seq: 0,
        },
    })
}

pub(crate) async fn subscribe_session(
    supervisor: &Supervisor,
    request: SubscribeRequest,
) -> Result<SessionSubscription, AgentApiError> {
    let hub = supervisor.session_hub(&request.session_id).await;
    let cursor = request.after.clone().unwrap_or_else(|| hub.cursor());
    let subscription = hub.subscribe();
    Ok(SessionSubscription {
        session_id: request.session_id,
        cursor: cursor.clone(),
        output: crate::runtime::events::merged_output_stream(subscription, cursor),
    })
}
