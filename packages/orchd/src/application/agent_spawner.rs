// ---- AgentSpawner impl on Supervisor ----

use std::time::{Duration, Instant};

use crate::domain::tasks::task::HostTaskContext;
use crate::ports::agent_spawner::{AgentReport, AgentSpawner};

use super::supervisor::{PendingStream, Supervisor};
use super::utils::generate_task_id;

#[async_trait::async_trait]
impl AgentSpawner for Supervisor {
    async fn spawn(
        &self,
        agent_id: &str,
        prompt: &str,
        host_context: HostTaskContext,
    ) -> Option<AgentReport> {
        const TIMEOUT: Duration = Duration::from_secs(300);

        let task_id = self.spawn_detached(agent_id, prompt, host_context).await;
        let started = Instant::now();

        loop {
            if let Some(report) = self.state.task_results.lock().await.get(&task_id).cloned() {
                return Some(report);
            }
            if started.elapsed() > TIMEOUT {
                return Some(AgentReport {
                    text: format!("Task spawned as detached (id: {task_id}). Use await_task to collect results."),
                    status: "detached".into(),
                    total_steps: 0,
                    task_id: Some(task_id),
                });
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn spawn_detached(
        &self,
        agent_id: &str,
        prompt: &str,
        host_context: HostTaskContext,
    ) -> String {
        let task_id = generate_task_id();
        let spec = match self.state.agent_specs.read().await.get(agent_id).cloned() {
            Some(s) => s,
            None => return task_id,
        };

        let stream = self.spawn_agent_stream(
            spec, prompt.to_string(), Some(host_context), None,
        ).await;

        self.state.pending_streams.lock().await.push(PendingStream {
            agent_id: task_id.clone(),
            stream,
        });

        task_id
    }

    async fn poll_task(&self, task_id: &str, timeout_ms: Option<u64>) -> Option<AgentReport> {
        // Immediate poll first
        if let Some(report) = self.state.task_results.lock().await.get(task_id).cloned() {
            return Some(report);
        }
        // Optional blocking wait
        let timeout = match timeout_ms {
            Some(ms) if ms > 0 => Duration::from_millis(ms),
            _ => return None,
        };
        let started = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if let Some(report) = self.state.task_results.lock().await.get(task_id).cloned() {
                return Some(report);
            }
            if started.elapsed() > timeout {
                return None;
            }
        }
    }

    async fn steer_task(&self, task_id: &str, message: &str) -> bool {
        let handles = self.state.handles.read().await;
        if let Some(handle) = handles.get(task_id) {
            handle.steer_tx.send(crate::domain::tasks::steering::SteerMessage {
                source_task_id: String::new(),
                source_agent_id: String::new(),
                message: message.to_string(),
            }).is_ok()
        } else {
            false
        }
    }
}
