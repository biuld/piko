use futures_util::StreamExt;
use piko_protocol::agent_runtime::{CreateTaskRequest, TaskHandle, TaskStatus};

use crate::api::AgentApiError;
use crate::domain::tasks::task::{AgentTask, TaskSource};

use super::super::supervision::utils::generate_task_id;
use super::super::supervision::{Supervisor, TaskRegistry, spawn_registered_agent_task};

pub(crate) async fn create_task(
    supervisor: &Supervisor,
    request: CreateTaskRequest,
) -> Result<TaskHandle, AgentApiError> {
    if supervisor.persist_sink().await.is_none() {
        return Err(AgentApiError::PersistenceUnavailable);
    }
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
    let output_hub = supervisor.session_hub(&session_id).await;
    let before_create = output_hub.cursor();

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
        history: request
            .resume
            .as_ref()
            .map(|resume| resume.transcript.clone()),
        resume: request.resume.clone(),
        host_context: Some(host_context),
    };

    spawn_registered_agent_task(supervisor, spec, task, true).await;

    let created = if request.resume.is_some() {
        true
    } else {
        let subscription = output_hub
            .subscribe(&before_create)
            .await
            .map_err(|_| AgentApiError::SnapshotRequired)?;
        let mut output = crate::runtime::events::merged_output_stream(
            subscription,
            before_create,
            Some(task_id.clone()),
        );
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while let Some(item) = output.next().await {
                match item {
                    Ok(envelope)
                        if matches!(
                            envelope.output,
                            piko_protocol::agent_runtime::SessionOutput::Event(
                                piko_protocol::agent_runtime::SessionEventEnvelope {
                                    event:
                                        piko_protocol::agent_runtime::SessionEvent::TaskChanged {
                                            snapshot: piko_protocol::agent_runtime::TaskSnapshot {
                                                status: TaskStatus::Created,
                                                ..
                                            },
                                        },
                                    ..
                                }
                            )
                        ) =>
                    {
                        return true;
                    }
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }
            false
        })
        .await
        .unwrap_or(false)
    };
    if !created {
        supervisor.state.registry.cleanup_runtime(&task_id).await;
        return Err(AgentApiError::PersistenceFailed(
            "task creation was not durably committed".into(),
        ));
    }

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
