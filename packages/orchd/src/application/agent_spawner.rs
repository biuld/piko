use std::time::{Duration, Instant};

use futures_core::Stream;
use std::pin::Pin;

use crate::domain::tasks::task::HostTaskContext;
use crate::ports::agent_spawner::{AgentReport, AgentSpawner};
use crate::runtime::types::{TaskControlMessage, TaskSteerMessage};
use piko_protocol::ServerMessage;

use super::supervisor::Supervisor;
use super::task_driver::spawn_task_driver;
use super::utils::generate_task_id;

impl Supervisor {
    pub(crate) async fn start_task_driver(
        &self,
        task_id: String,
        stream: Pin<Box<dyn Stream<Item = ServerMessage> + Send>>,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) {
        spawn_task_driver(std::sync::Arc::clone(&self.state), task_id, stream, senders);
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
        const TIMEOUT: Duration = Duration::from_secs(300);
        let task_id = generate_task_id();
        let spec = self.ensure_agent(agent_id).await;
        let stream = self
            .spawn_agent_stream(
                spec,
                prompt.to_string(),
                Some(host_context.clone()),
                source_agent_id,
                parent_task_id.clone(),
                Some(task_id.clone()),
                true,
            )
            .await;
        self.start_task_driver(task_id.clone(), stream, senders)
            .await;

        if let Some(report) = self.wait_for_task_result(&task_id, TIMEOUT).await {
            return Some(report);
        }

        Some(AgentReport {
            text: format!(
                "Task spawned as detached (id: {task_id}). Use await_task to collect results."
            ),
            status: "detached".into(),
            total_steps: 0,
            task_id: Some(task_id),
        })
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
        let task_id = generate_task_id();
        let spec = self.ensure_agent(agent_id).await;

        let stream = self
            .spawn_agent_stream(
                spec,
                prompt.to_string(),
                Some(host_context.clone()),
                source_agent_id,
                parent_task_id.clone(),
                Some(task_id.clone()),
                true,
            )
            .await;

        self.start_task_driver(task_id.clone(), stream, senders)
            .await;

        task_id
    }

    async fn poll_task(&self, task_id: &str) -> Option<AgentReport> {
        self.state.registry.task_result(task_id).await
    }

    async fn steer_task(
        &self,
        task_id: &str,
        message: &str,
        source_task_id: Option<String>,
        source_agent_id: Option<String>,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> bool {
        if let Some(handle) = self.state.registry.handle(task_id).await {
            let sent = handle
                .control_tx
                .send(TaskControlMessage::Steer(TaskSteerMessage {
                    source_task_id: source_task_id.unwrap_or_default(),
                    source_agent_id: source_agent_id.unwrap_or_default(),
                    message: message.to_string(),
                    senders,
                }))
                .is_ok();
            if sent {
                self.state.registry.clear_task_result(task_id).await;
            }
            sent
        } else {
            false
        }
    }

    async fn list_agents(&self) -> Vec<crate::domain::agents::spec::AgentSpec> {
        let specs = self.state.agent_specs.read().await;
        let mut list: Vec<_> = specs.values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }
}
