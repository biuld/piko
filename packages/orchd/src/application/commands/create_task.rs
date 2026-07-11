use crate::api::AgentRuntime;
use piko_protocol::agent_runtime::{CreateTaskRequest, TaskHandle};

use crate::api::AgentApiError;
use crate::application::service::AgentRuntimeService;

pub async fn create_task(
    service: &AgentRuntimeService,
    request: CreateTaskRequest,
) -> Result<TaskHandle, AgentApiError> {
    service.create_task(request).await
}
