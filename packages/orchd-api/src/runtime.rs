use async_trait::async_trait;

use piko_protocol::agent_runtime::{
    CreateTaskRequest, SessionRuntimeSnapshot, SubmitTaskInput, SubscribeRequest,
    TaskControlRequest, TaskHandle, TaskSnapshot,
};

use crate::error::AgentApiError;
use crate::request::InputReceipt;
use crate::stream::SessionSubscription;

/// Unified command, snapshot, and observation surface for every agent task.
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    async fn create_task(&self, request: CreateTaskRequest) -> Result<TaskHandle, AgentApiError>;

    async fn submit_input(&self, request: SubmitTaskInput) -> Result<InputReceipt, AgentApiError>;

    async fn control_task(
        &self,
        request: TaskControlRequest,
    ) -> Result<TaskSnapshot, AgentApiError>;

    async fn task_snapshot(&self, task_id: String) -> Result<TaskSnapshot, AgentApiError>;

    async fn session_snapshot(
        &self,
        session_id: String,
    ) -> Result<SessionRuntimeSnapshot, AgentApiError>;

    async fn subscribe_session(
        &self,
        request: SubscribeRequest,
    ) -> Result<SessionSubscription, AgentApiError>;
}
