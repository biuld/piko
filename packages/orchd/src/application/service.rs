use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{
    CreateTaskRequest, InputReceipt, SessionRuntimeSnapshot, SubmitTaskInput, SubscribeRequest,
    TaskControlRequest, TaskHandle, TaskSnapshot,
};

use crate::api::{AgentApiError, AgentRuntime, SessionSubscription};
use crate::domain::tasks::task::HostTaskContext;
use crate::runtime::task::input::build_user_input;
use piko_protocol::agent_runtime::{InputSource, TaskMode};

use super::commands::{control_task, create_task, submit_input};
use super::queries::{list_tasks, subscribe_session, task_snapshot};
use super::supervision::Supervisor;
use super::supervision::utils::generate_task_id;

/// Agent API facade over the existing supervisor runtime.
pub struct AgentRuntimeService {
    supervisor: Arc<Supervisor>,
}

impl AgentRuntimeService {
    pub fn new(supervisor: Arc<Supervisor>) -> Self {
        Self { supervisor }
    }

    pub fn from_supervisor(supervisor: &Supervisor) -> Self {
        Self::new(Arc::new(Supervisor::with_state(Arc::clone(
            &supervisor.state,
        ))))
    }

    pub fn supervisor(&self) -> &Supervisor {
        &self.supervisor
    }

    pub fn runtime_for(supervisor: &Supervisor) -> Self {
        Self::from_supervisor(supervisor)
    }

    /// Subscribe to a session, then create or reuse the root task and submit the first user input.
    pub async fn start_root_turn(
        &self,
        session_id: &str,
        source_turn_id: &str,
        work_id: &str,
        agent_id: &str,
        prompt: &str,
        resume: Option<piko_protocol::agent_runtime::TaskResumeState>,
        resume_task_id: Option<&str>,
    ) -> Result<SessionSubscription, AgentApiError> {
        let subscription = subscribe_session::subscribe_session(
            &self.supervisor,
            SubscribeRequest {
                session_id: session_id.to_string(),
                task_id: None,
                after: None,
            },
        )
        .await?;

        if let Some(task_id) = self
            .supervisor
            .state
            .registry
            .active_root_task_for_agent(agent_id, session_id)
            .await
        {
            match submit_input::submit_input(
                &self.supervisor,
                build_user_input(
                    session_id,
                    &task_id,
                    work_id,
                    MessageContent::String(prompt.to_string()),
                    InputSource::User,
                    Some(source_turn_id.to_string()),
                ),
            )
            .await
            {
                Ok(_) => return Ok(subscription),
                Err(error @ AgentApiError::PersistenceFailed(_)) => return Err(error),
                Err(_) => {
                    self.supervisor
                        .state
                        .registry
                        .cleanup_runtime(&task_id)
                        .await;
                }
            }
        }

        let _ = self.supervisor.ensure_agent(agent_id).await;
        let task_id = resume_task_id
            .map(str::to_string)
            .unwrap_or_else(generate_task_id);
        let request = CreateTaskRequest {
            request_id: format!("req_{}", uuid::Uuid::new_v4()),
            session_id: session_id.to_string(),
            task_id: Some(task_id.clone()),
            agent_id: agent_id.to_string(),
            parent_task_id: None,
            source: InputSource::User,
            mode: TaskMode::Attached,
            host_context: HostTaskContext::new(session_id),
            resume,
        };

        create_task::create_task(&self.supervisor, request).await?;
        submit_input::submit_input(
            &self.supervisor,
            build_user_input(
                session_id,
                &task_id,
                work_id,
                MessageContent::String(prompt.to_string()),
                InputSource::User,
                Some(source_turn_id.to_string()),
            ),
        )
        .await?;

        Ok(subscription)
    }
}

#[async_trait]
impl AgentRuntime for AgentRuntimeService {
    async fn create_task(&self, request: CreateTaskRequest) -> Result<TaskHandle, AgentApiError> {
        create_task::create_task(&self.supervisor, request).await
    }

    async fn submit_input(&self, request: SubmitTaskInput) -> Result<InputReceipt, AgentApiError> {
        submit_input::submit_input(&self.supervisor, request).await
    }

    async fn control_task(
        &self,
        request: TaskControlRequest,
    ) -> Result<TaskSnapshot, AgentApiError> {
        control_task::control_task(&self.supervisor, request).await
    }

    async fn task_snapshot(&self, task_id: String) -> Result<TaskSnapshot, AgentApiError> {
        task_snapshot::task_snapshot(&self.supervisor, task_id).await
    }

    async fn session_snapshot(
        &self,
        session_id: String,
    ) -> Result<SessionRuntimeSnapshot, AgentApiError> {
        list_tasks::list_tasks(&self.supervisor, session_id).await
    }

    async fn subscribe_session(
        &self,
        request: SubscribeRequest,
    ) -> Result<SessionSubscription, AgentApiError> {
        subscribe_session::subscribe_session(&self.supervisor, request).await
    }
}
