use piko_protocol::agent_runtime::TaskControlRequest;

use crate::api::AgentApiError;
use crate::runtime::task::{TaskControlEnvelope, TaskInputEnvelope, TaskMailboxMessage};

use super::super::queries::task_snapshot;
use super::super::supervision::Supervisor;

pub(crate) async fn control_task(
    supervisor: &Supervisor,
    request: TaskControlRequest,
) -> Result<piko_protocol::agent_runtime::TaskSnapshot, AgentApiError> {
    let (request_id, task_id) = match &request {
        TaskControlRequest::Close {
            request_id,
            task_id,
        }
        | TaskControlRequest::Reopen {
            request_id,
            task_id,
        }
        | TaskControlRequest::CancelWork {
            request_id,
            task_id,
            ..
        }
        | TaskControlRequest::Terminate {
            request_id,
            task_id,
        } => (request_id.clone(), task_id.clone()),
    };

    if let Some(stored) = supervisor
        .state
        .registry
        .lookup_control_request(&request_id)
        .await
    {
        if stored == request {
            return task_snapshot::task_snapshot(supervisor, task_id).await;
        }
        return Err(AgentApiError::IdempotencyConflict);
    }

    let handle = supervisor
        .state
        .registry
        .handle(&task_id)
        .await
        .ok_or(AgentApiError::TaskNotFound)?;

    match &request {
        TaskControlRequest::Terminate { .. } => {
            handle.cancel.cancel();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                let snapshot = task_snapshot::task_snapshot(supervisor, task_id.clone()).await?;
                if snapshot.status == piko_protocol::agent_runtime::TaskStatus::Terminated {
                    supervisor
                        .state
                        .registry
                        .record_control_request(request)
                        .await;
                    return Ok(snapshot);
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(AgentApiError::RuntimeUnavailable);
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
        TaskControlRequest::CancelWork { work_id, .. } => {
            let active = supervisor
                .state
                .registry
                .active_work_snapshot(&task_id)
                .await;
            if active.as_ref().is_none_or(|work| &work.work_id != work_id) {
                return Err(AgentApiError::InvalidState);
            }
            let (envelope, ack_rx) = TaskControlEnvelope::acknowledged(request.clone());
            handle
                .control_tx
                .send(TaskMailboxMessage::Control(envelope))
                .map_err(|_| AgentApiError::RuntimeUnavailable)?;
            await_control_ack(ack_rx).await?;
            supervisor.state.registry.cancel_active_work(&task_id).await;
        }
        _ => {
            let (envelope, ack_rx) = TaskControlEnvelope::acknowledged(request.clone());
            if handle
                .control_tx
                .send(TaskMailboxMessage::Control(envelope))
                .is_err()
            {
                return Err(AgentApiError::RuntimeUnavailable);
            }
            await_control_ack(ack_rx).await?;
        }
    }

    let expected_status = match &request {
        TaskControlRequest::Close { .. } => Some(piko_protocol::agent_runtime::TaskStatus::Closed),
        TaskControlRequest::Reopen { .. } | TaskControlRequest::CancelWork { .. } => {
            Some(piko_protocol::agent_runtime::TaskStatus::Idle)
        }
        TaskControlRequest::Terminate { .. } => None,
    };
    if let Some(expected) = expected_status {
        await_task_status(supervisor, &task_id, expected).await?;
    }
    let snapshot = task_snapshot::task_snapshot(supervisor, task_id).await?;
    supervisor
        .state
        .registry
        .record_control_request(request)
        .await;
    Ok(snapshot)
}

async fn await_task_status(
    supervisor: &Supervisor,
    task_id: &str,
    expected: piko_protocol::agent_runtime::TaskStatus,
) -> Result<(), AgentApiError> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let snapshot = task_snapshot::task_snapshot(supervisor, task_id.to_string()).await?;
        if snapshot.status == expected {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AgentApiError::RuntimeUnavailable);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

async fn await_control_ack(
    ack_rx: tokio::sync::oneshot::Receiver<Result<(), String>>,
) -> Result<(), AgentApiError> {
    match tokio::time::timeout(std::time::Duration::from_secs(30), ack_rx).await {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(_))) => Err(AgentApiError::InvalidState),
        _ => Err(AgentApiError::RuntimeUnavailable),
    }
}
