use async_trait::async_trait;
use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{CreateTaskRequest, InputSource, TaskHandle, TaskMode};

use crate::application::service::AgentRuntimeService;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::HostTaskContext;
use crate::ports::agent_spawner::AgentReport;
use crate::runtime::task::input::build_user_input;

/// Restricted task-control capability for spawn/steer tools.
#[async_trait]
pub trait TaskControlPort: Send + Sync {
    async fn create_child_with_input(
        &self,
        request: CreateTaskRequest,
        prompt: &str,
    ) -> Result<TaskHandle, crate::api::AgentApiError>;

    async fn spawn_and_wait(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
    ) -> Option<AgentReport>;

    async fn spawn_detached(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
    ) -> Result<String, crate::api::AgentApiError>;

    async fn steer_task(
        &self,
        task_id: &str,
        message: &str,
        source_task_id: Option<String>,
        source_agent_id: Option<String>,
    ) -> bool;

    async fn poll_task(&self, task_id: &str) -> Option<AgentReport>;

    async fn list_agents(&self) -> Vec<AgentSpec>;
}

pub struct TaskControlPortImpl {
    runtime: AgentRuntimeService,
    state: Arc<crate::application::supervision::SupervisorState>,
}

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::api::AgentRuntime;

impl TaskControlPortImpl {
    pub fn new(supervisor: Arc<crate::application::Supervisor>) -> Self {
        Self {
            runtime: AgentRuntimeService::new(supervisor.clone()),
            state: Arc::clone(&supervisor.state),
        }
    }

    async fn wait_for_task_result(&self, task_id: &str, timeout: Duration) -> Option<AgentReport> {
        let started = Instant::now();
        loop {
            if let Some(report) = self.state.registry.task_result(task_id).await {
                return Some(report);
            }
            if started.elapsed() >= timeout {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
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
        let handle = self.runtime.create_task(request.clone()).await?;
        self.runtime
            .submit_input(build_user_input(
                &request.host_context.session_id,
                &handle.task_id,
                &new_work_id(),
                MessageContent::String(prompt.to_string()),
                request.source,
            ))
            .await?;
        Ok(handle)
    }

    async fn spawn_and_wait(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
    ) -> Option<AgentReport> {
        let source = child_input_source(source_agent_id, parent_task_id.clone());
        let request = create_child_request(
            &host_context.session_id,
            &host_context.turn_id,
            agent_id,
            parent_task_id,
            source,
        );
        let handle = self.create_child_with_input(request, prompt).await.ok()?;
        const TIMEOUT: Duration = Duration::from_secs(300);
        if let Some(report) = self.wait_for_task_result(&handle.task_id, TIMEOUT).await {
            return Some(report);
        }
        Some(AgentReport {
            text: format!(
                "Task spawned as detached (id: {}). Use poll_task to collect results.",
                handle.task_id
            ),
            status: "detached".into(),
            total_steps: 0,
            task_id: Some(handle.task_id),
        })
    }

    async fn spawn_detached(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
    ) -> Result<String, crate::api::AgentApiError> {
        let source = child_input_source(source_agent_id, parent_task_id.clone());
        let request = create_child_request(
            &host_context.session_id,
            &host_context.turn_id,
            agent_id,
            parent_task_id,
            source,
        );
        Ok(self.create_child_with_input(request, prompt).await?.task_id)
    }

    async fn steer_task(
        &self,
        task_id: &str,
        message: &str,
        source_task_id: Option<String>,
        source_agent_id: Option<String>,
    ) -> bool {
        let session_id = self
            .state
            .registry
            .task_session(task_id)
            .await
            .unwrap_or_else(|| self.state.run_id.clone());
        let source = match (source_task_id, source_agent_id) {
            (Some(task_id), Some(agent_id)) => InputSource::Task { task_id, agent_id },
            _ => InputSource::User,
        };
        self.runtime
            .submit_input(build_user_input(
                &session_id,
                task_id,
                &new_work_id(),
                MessageContent::String(message.to_string()),
                source,
            ))
            .await
            .is_ok()
    }

    async fn poll_task(&self, task_id: &str) -> Option<AgentReport> {
        self.state.registry.task_result(task_id).await
    }

    async fn list_agents(&self) -> Vec<AgentSpec> {
        let specs = self.state.agent_specs.read().await;
        let mut list: Vec<_> = specs.values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
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
        initial_history: None,
    }
}

fn child_input_source(
    source_agent_id: Option<String>,
    parent_task_id: Option<String>,
) -> InputSource {
    match (source_agent_id, parent_task_id) {
        (Some(agent_id), Some(task_id)) => InputSource::Task { task_id, agent_id },
        _ => InputSource::User,
    }
}

fn new_work_id() -> String {
    format!("work_{}", uuid::Uuid::new_v4())
}
