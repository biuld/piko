use crate::api::AgentRuntime;
use piko_protocol::agent_runtime::{InputReceipt, SubmitTaskInput};

use crate::api::AgentApiError;
use crate::application::service::AgentRuntimeService;

pub async fn submit_input(
    service: &AgentRuntimeService,
    request: SubmitTaskInput,
) -> Result<InputReceipt, AgentApiError> {
    service.submit_input(request).await
}
