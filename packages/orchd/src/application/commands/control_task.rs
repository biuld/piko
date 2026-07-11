use piko_protocol::agent_runtime::TaskControlRequest;

use crate::api::AgentApiError;
use crate::runtime::types::TaskMailboxMessage;

use super::super::queries::task_snapshot;
use super::super::supervision::Supervisor;

pub(crate) async fn control_task(
    supervisor: &Supervisor,
    request: TaskControlRequest,
) -> Result<piko_protocol::agent_runtime::TaskSnapshot, AgentApiError> {
    let task_id = match &request {
        TaskControlRequest::Close { task_id, .. }
        | TaskControlRequest::Reopen { task_id, .. }
        | TaskControlRequest::CancelWork { task_id, .. }
        | TaskControlRequest::Terminate { task_id, .. } => task_id.clone(),
    };

    let handle = supervisor
        .state
        .registry
        .handle(&task_id)
        .await
        .ok_or(AgentApiError::TaskNotFound)?;

    match &request {
        TaskControlRequest::Terminate { .. } => handle.cancel.cancel(),
        _ => {
            if handle
                .control_tx
                .send(TaskMailboxMessage::Control(request.clone()))
                .is_err()
            {
                return Err(AgentApiError::RuntimeUnavailable);
            }
        }
    }

    task_snapshot::task_snapshot(supervisor, task_id).await
}
