use piko_protocol::agent_runtime::{
    InputDelivery, InputDisposition, InputReceipt, SubmitTaskInput, WorkSnapshot, WorkStatus,
};

use crate::api::AgentApiError;
use crate::runtime::types::{TaskInputEnvelope, TaskMailboxMessage};

use super::super::supervision::Supervisor;

pub(crate) async fn submit_input(
    supervisor: &Supervisor,
    request: SubmitTaskInput,
) -> Result<InputReceipt, AgentApiError> {
    if let Some(stored) = supervisor
        .state
        .registry
        .lookup_input_receipt(&request.task_id, &request.request_id)
        .await
    {
        if stored.input == request {
            return Ok(InputReceipt {
                disposition: InputDisposition::Duplicate,
                ..stored.receipt
            });
        }
        return Err(AgentApiError::IdempotencyConflict);
    }

    let handle = supervisor
        .state
        .registry
        .handle(&request.task_id)
        .await
        .ok_or(AgentApiError::TaskNotFound)?;

    let was_busy = supervisor
        .state
        .registry
        .active_work_snapshot(&request.task_id)
        .await
        .is_some_and(|work| matches!(work.status, WorkStatus::Accepted | WorkStatus::Running));
    if was_busy && matches!(request.delivery, InputDelivery::Immediate) {
        return Err(AgentApiError::InputRejected);
    }
    supervisor
        .state
        .registry
        .clear_task_result(&request.task_id)
        .await;

    let registered_session = supervisor
        .state
        .registry
        .task_session(&request.task_id)
        .await;
    if registered_session.is_some_and(|session_id| session_id != request.session_id) {
        return Err(AgentApiError::SessionMismatch);
    }

    supervisor
        .persist_sink()
        .await
        .ok_or(AgentApiError::PersistenceUnavailable)?;
    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
    let envelope = TaskInputEnvelope {
        input: request.clone(),
        ack_tx: Some(ack_tx),
    };

    let sent = handle
        .control_tx
        .send(TaskMailboxMessage::Input(envelope))
        .is_ok();
    if !sent {
        return Err(AgentApiError::RuntimeUnavailable);
    }

    match tokio::time::timeout(std::time::Duration::from_secs(30), ack_rx).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(message))) => return Err(AgentApiError::PersistenceFailed(message)),
        _ => return Err(AgentApiError::RuntimeUnavailable),
    }

    let receipt = InputReceipt {
        request_id: request.request_id.clone(),
        task_id: request.task_id.clone(),
        work_id: request.work_id.clone(),
        message_id: request.message_id.clone(),
        disposition: if was_busy {
            InputDisposition::Queued
        } else {
            InputDisposition::Accepted
        },
    };
    supervisor
        .state
        .registry
        .record_input_receipt(&request, receipt.clone())
        .await;

    supervisor
        .state
        .registry
        .set_active_work(
            &request.task_id,
            WorkSnapshot {
                work_id: request.work_id.clone(),
                status: WorkStatus::Accepted,
                source_turn_id: request.source_turn_id.clone(),
            },
        )
        .await;

    Ok(receipt)
}
