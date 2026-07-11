use piko_protocol::agent_runtime::{
    InputDelivery, InputDisposition, InputReceipt, SubmitTaskInput,
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

    let registered_session = supervisor
        .state
        .registry
        .task_session(&request.task_id)
        .await;
    if registered_session.is_some_and(|session_id| session_id != request.session_id) {
        return Err(AgentApiError::SessionMismatch);
    }

    let persist_sink = supervisor.persist_sink().await;
    let (envelope, ack_rx) = if persist_sink.is_some() {
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        (
            TaskInputEnvelope {
                input: request.clone(),
                ack_tx: Some(ack_tx),
            },
            Some(ack_rx),
        )
    } else {
        (TaskInputEnvelope::without_ack(request.clone()), None)
    };

    let sent = handle
        .control_tx
        .send(TaskMailboxMessage::Input(envelope))
        .is_ok();
    if !sent {
        return Err(AgentApiError::RuntimeUnavailable);
    }

    if let Some(ack_rx) = ack_rx {
        match tokio::time::timeout(std::time::Duration::from_secs(30), ack_rx).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(message))) => return Err(AgentApiError::PersistenceFailed(message)),
            _ => return Err(AgentApiError::RuntimeUnavailable),
        }
    }

    if matches!(request.delivery, InputDelivery::AfterCurrentStep) {
        supervisor
            .state
            .registry
            .clear_task_result(&request.task_id)
            .await;
    }

    let receipt = InputReceipt {
        request_id: request.request_id.clone(),
        task_id: request.task_id.clone(),
        work_id: request.work_id.clone(),
        message_id: request.message_id.clone(),
        disposition: InputDisposition::Accepted,
    };
    supervisor
        .state
        .registry
        .record_input_receipt(&request, receipt.clone())
        .await;

    Ok(receipt)
}
