use std::sync::Arc;

use piko_protocol::agent_runtime::{CreateTaskRequest, TaskHandle, TaskStatus};

use crate::api::AgentApiError;
use crate::domain::tasks::task::{AgentTask, TaskSource};

use super::super::supervision::{
    Supervisor, TaskRegistry, spawn_registered_agent_stream, spawn_task_driver,
};
use super::super::utils::generate_task_id;

pub(crate) async fn create_task(
    supervisor: &Supervisor,
    request: CreateTaskRequest,
) -> Result<TaskHandle, AgentApiError> {
    if let Some(stored) = supervisor
        .state
        .registry
        .lookup_create_task(&request.request_id)
        .await
    {
        if TaskRegistry::create_requests_match(&stored.request, &request, &stored.handle.task_id) {
            return Ok(stored.handle);
        }
        return Err(AgentApiError::IdempotencyConflict);
    }

    let task_id = request.task_id.clone().unwrap_or_else(generate_task_id);
    let spec = supervisor.ensure_agent(&request.agent_id).await;
    let host_context = request.host_context.clone();
    let session_id = host_context.session_id.clone();

    let task = AgentTask {
        id: Some(task_id.clone()),
        target_agent_id: request.agent_id.clone(),
        prompt: String::new(),
        source: match &request.source {
            piko_protocol::agent_runtime::InputSource::Task {
                task_id: parent_task_id,
                agent_id,
            } => TaskSource::Agent {
                agent_id: agent_id.clone(),
                task_id: parent_task_id.clone(),
            },
            _ => TaskSource::User,
        },
        priority: None,
        parent_task_id: request.parent_task_id.clone(),
        history: request.initial_history.clone(),
        host_context: Some(host_context),
    };

    let stream = spawn_registered_agent_stream(
        supervisor,
        spec,
        task,
        matches!(
            request.mode,
            piko_protocol::agent_runtime::TaskMode::Attached
        ),
    )
    .await;
    spawn_task_driver(Arc::clone(&supervisor.state), task_id.clone(), stream);

    let handle = TaskHandle {
        session_id,
        task_id,
        agent_id: request.agent_id.clone(),
        status: TaskStatus::Created,
    };
    supervisor
        .state
        .registry
        .record_create_task(&request, handle.clone())
        .await;
    Ok(handle)
}
