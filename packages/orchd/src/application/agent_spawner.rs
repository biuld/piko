// ---- AgentSpawner impl on Supervisor ----

use std::time::{Duration, Instant};

use futures_core::Stream;
use futures_util::StreamExt;
use std::pin::Pin;
use tokio::sync::oneshot;

use crate::domain::tasks::task::HostTaskContext;
use crate::ports::agent_spawner::{AgentReport, AgentSpawner};
use piko_protocol::{ServerMessage, TaskEvent};

use super::supervisor::Supervisor;
use super::utils::generate_task_id;

impl Supervisor {
    async fn start_task_driver(
        &self,
        task_id: String,
        mut stream: Pin<Box<dyn Stream<Item = ServerMessage> + Send>>,
        result_tx: Option<oneshot::Sender<AgentReport>>,
    ) {
        let channel_bus = self.state.runtime_events.clone();
        let supervisor = Self {
            state: std::sync::Arc::clone(&self.state),
        };

        self.state
            .registered_task_ids
            .write()
            .await
            .insert(task_id.clone());

        tokio::spawn(async move {
            let mut result_tx = result_tx;
            while let Some(event) = stream.next().await {
                supervisor.observe_task_event(&event).await;

                let report = report_from_terminal_event(&event);
                channel_bus.send_event(&event).await;

                if let Some((task_id, report)) = report {
                    supervisor
                        .record_task_result(&task_id, report.clone())
                        .await;
                    if let Some(tx) = result_tx.take() {
                        let _ = tx.send(report);
                    }
                }
            }
        });
    }
}

fn report_from_terminal_event(event: &ServerMessage) -> Option<(String, AgentReport)> {
    match event {
        ServerMessage::Display(piko_protocol::DisplayEvent::TaskLifecycle(TaskEvent::Completed {
            task_id,
            summary,
            final_status,
            total_steps,
            ..
        })) => Some((
            task_id.clone(),
            AgentReport {
                text: summary.clone(),
                status: final_status.clone(),
                total_steps: *total_steps,
                task_id: None,
            },
        )),
        ServerMessage::Display(piko_protocol::DisplayEvent::TaskLifecycle(TaskEvent::Failed { task_id, error, .. })) => Some((
            task_id.clone(),
            AgentReport {
                text: error.clone(),
                status: "error".into(),
                total_steps: 0,
                task_id: None,
            },
        )),
        ServerMessage::Display(piko_protocol::DisplayEvent::TaskLifecycle(TaskEvent::Cancelled { task_id, .. })) => Some((
            task_id.clone(),
            AgentReport {
                text: "cancelled".into(),
                status: "cancelled".into(),
                total_steps: 0,
                task_id: None,
            },
        )),
        _ => None,
    }
}

#[async_trait::async_trait]
impl AgentSpawner for Supervisor {
    async fn spawn(
        &self,
        agent_id: &str,
        prompt: &str,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
    ) -> Option<AgentReport> {
        const TIMEOUT: Duration = Duration::from_secs(300);
        let task_id = generate_task_id();
        let spec = self.ensure_agent(agent_id).await;
        let stream = self
            .spawn_agent_stream(
                spec,
                prompt.to_string(),
                Some(host_context.clone()),
                None,
                parent_task_id.clone(),
                Some(task_id.clone()),
            )
            .await;
        let (tx, rx) = oneshot::channel();
        self.start_task_driver(task_id.clone(), stream, Some(tx))
            .await;

        if let Ok(Ok(report)) = tokio::time::timeout(TIMEOUT, rx).await {
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
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
    ) -> String {
        let task_id = generate_task_id();
        let spec = self.ensure_agent(agent_id).await;

        let stream = self
            .spawn_agent_stream(
                spec,
                prompt.to_string(),
                Some(host_context.clone()),
                None,
                parent_task_id.clone(),
                Some(task_id.clone()),
            )
            .await;

        self.start_task_driver(task_id.clone(), stream, None).await;

        task_id
    }

    async fn poll_task(&self, task_id: &str, timeout_ms: Option<u64>) -> Option<AgentReport> {
        // Immediate poll first
        if let Some(report) = self.state.task_results.lock().await.get(task_id).cloned() {
            return Some(report);
        }

        let is_registered = self
            .state
            .registered_task_ids
            .read()
            .await
            .contains(task_id);

        // Optional blocking wait
        let timeout = match timeout_ms {
            Some(0) => return None,
            Some(ms) if ms > 0 => Duration::from_millis(ms),
            _ if is_registered => Duration::from_secs(5),
            _ => return None,
        };
        let started = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(10)).await;
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
            handle
                .steer_tx
                .send(crate::domain::tasks::steering::SteerMessage {
                    source_task_id: String::new(),
                    source_agent_id: String::new(),
                    message: message.to_string(),
                })
                .is_ok()
        } else {
            false
        }
    }
}
