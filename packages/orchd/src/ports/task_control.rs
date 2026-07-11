use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{
    CreateTaskRequest, InputSource, SubmitTaskInput, TaskHandle, TaskMode,
};

use crate::application::commands::{create_task, submit_input};
use crate::application::service::AgentRuntimeService;
use crate::domain::tasks::task::HostTaskContext;
use crate::runtime::orchestrator::input::build_user_input;

/// Restricted task-control capability for spawn/steer tools.
#[async_trait]
pub trait TaskControlPort: Send + Sync {
    async fn create_child_with_input(
        &self,
        request: CreateTaskRequest,
        prompt: &str,
    ) -> Result<TaskHandle, crate::api::AgentApiError>;

    async fn steer_child(&self, input: SubmitTaskInput) -> Result<(), crate::api::AgentApiError>;
}

pub struct TaskControlPortImpl {
    runtime: AgentRuntimeService,
}

impl TaskControlPortImpl {
    pub fn new(supervisor: Arc<crate::application::Supervisor>) -> Self {
        Self {
            runtime: AgentRuntimeService::new(supervisor),
        }
    }
}

#[async_trait]
impl TaskControlPort for TaskControlPortImpl {
    async fn create_child_with_input(
        &self,
        request: CreateTaskRequest,
        prompt: &str,
    ) -> Result<TaskHandle, crate::api::AgentApiError> {
        let handle = create_task(&self.runtime, request.clone()).await?;
        submit_input(
            &self.runtime,
            build_user_input(
                &request.host_context.session_id,
                &handle.task_id,
                &request.host_context.turn_id,
                MessageContent::String(prompt.to_string()),
                request.source,
            ),
        )
        .await?;
        Ok(handle)
    }

    async fn steer_child(&self, input: SubmitTaskInput) -> Result<(), crate::api::AgentApiError> {
        submit_input(&self.runtime, input).await.map(|_| ())
    }
}

pub fn create_child_request(
    session_id: &str,
    turn_id: &str,
    agent_id: &str,
    parent_task_id: Option<String>,
    source: InputSource,
) -> CreateTaskRequest {
    CreateTaskRequest {
        request_id: format!("req_{}", uuid::Uuid::new_v4()),
        session_id: session_id.to_string(),
        task_id: None,
        agent_id: agent_id.to_string(),
        parent_task_id,
        source,
        mode: TaskMode::Attached,
        host_context: HostTaskContext {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
        },
    }
}
