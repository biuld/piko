use piko_protocol::agent_runtime::{TaskSnapshot, TaskStatus, WorkSnapshot, WorkStatus};

use crate::api::AgentApiError;

use super::super::supervision::Supervisor;

pub(crate) async fn task_snapshot(
    supervisor: &Supervisor,
    task_id: String,
) -> Result<TaskSnapshot, AgentApiError> {
    let tasks = supervisor.state.registry.tasks_snapshot().await;
    let task = tasks.get(&task_id).ok_or(AgentApiError::TaskNotFound)?;
    let session_id = supervisor
        .state
        .registry
        .task_session(&task_id)
        .await
        .unwrap_or_else(|| supervisor.state.run_id.clone());

    Ok(TaskSnapshot {
        session_id,
        task_id: task.id.clone(),
        agent_id: task.target_agent_id.clone(),
        parent_task_id: task.parent_task_id.clone(),
        status: map_task_status(&task.status),
        active_work: supervisor
            .state
            .registry
            .active_work_snapshot(&task_id)
            .await,
    })
}

pub(crate) fn map_task_status(status: &piko_protocol::agents::AgentTaskStatus) -> TaskStatus {
    use piko_protocol::agents::AgentTaskStatus;
    match status {
        AgentTaskStatus::Queued => TaskStatus::Created,
        AgentTaskStatus::Running => TaskStatus::Running,
        AgentTaskStatus::Idle => TaskStatus::Idle,
        AgentTaskStatus::Closed => TaskStatus::Closed,
        AgentTaskStatus::Completed | AgentTaskStatus::Cancelled => TaskStatus::Terminated,
        AgentTaskStatus::Failed => TaskStatus::Failed,
    }
}

pub(crate) fn active_work_snapshot(
    status: &piko_protocol::agents::AgentTaskStatus,
    work_id: &str,
) -> Option<WorkSnapshot> {
    use piko_protocol::agents::AgentTaskStatus;
    match status {
        AgentTaskStatus::Running => Some(WorkSnapshot {
            work_id: work_id.to_string(),
            status: WorkStatus::Running,
            source_turn_id: None,
        }),
        AgentTaskStatus::Queued => Some(WorkSnapshot {
            work_id: work_id.to_string(),
            status: WorkStatus::Accepted,
            source_turn_id: None,
        }),
        _ => None,
    }
}
