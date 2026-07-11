use crate::domain::tasks::task::HostTaskContext;
use crate::ports::agent_spawner::{AgentReport, AgentSpawner};

use super::supervisor::Supervisor;

#[async_trait::async_trait]
impl AgentSpawner for Supervisor {
    async fn spawn(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> Option<AgentReport> {
        let port = self.state.task_control.read().await.clone()?;
        port.spawn_and_wait(
            agent_id,
            prompt,
            source_agent_id,
            parent_task_id,
            host_context,
            senders,
        )
        .await
    }

    async fn spawn_detached(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> String {
        let port = match self.state.task_control.read().await.clone() {
            Some(port) => port,
            None => return String::new(),
        };
        port.spawn_detached(
            agent_id,
            prompt,
            source_agent_id,
            parent_task_id,
            host_context,
            senders,
        )
        .await
        .unwrap_or_default()
    }

    async fn poll_task(&self, task_id: &str) -> Option<AgentReport> {
        let port = self.state.task_control.read().await.clone()?;
        port.poll_task(task_id).await
    }

    async fn steer_task(
        &self,
        task_id: &str,
        message: &str,
        source_task_id: Option<String>,
        source_agent_id: Option<String>,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> bool {
        let port = match self.state.task_control.read().await.clone() {
            Some(port) => port,
            None => return false,
        };
        port.steer_task(task_id, message, source_task_id, source_agent_id, senders)
            .await
    }

    async fn list_agents(&self) -> Vec<crate::domain::agents::spec::AgentSpec> {
        let port = match self.state.task_control.read().await.clone() {
            Some(port) => port,
            None => return Vec::new(),
        };
        port.list_agents().await
    }
}
