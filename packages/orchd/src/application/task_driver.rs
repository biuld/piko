use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;
use futures_util::StreamExt;
use tokio::sync::oneshot;

use crate::ports::agent_spawner::AgentReport;
use crate::runtime::dispatch::DispatchSenders;
use piko_protocol::{ServerMessage, TaskEvent};

use super::supervisor::SupervisorState;

pub(crate) fn spawn_task_driver(
    state: Arc<SupervisorState>,
    mut stream: Pin<Box<dyn Stream<Item = ServerMessage> + Send>>,
    result_tx: Option<oneshot::Sender<AgentReport>>,
    senders: Option<DispatchSenders>,
) {
    tokio::spawn(async move {
        use crate::runtime::dispatch::{
            display_events_from_server_message, lifecycle_events_from_server_message,
            persist_events_from_server_message,
        };

        let mut result_tx = result_tx;
        let mut senders = senders;

        while let Some(event) = stream.next().await {
            if let ServerMessage::TaskLifecycle(task_event) = &event {
                state.registry.apply_task_event(task_event).await;
            }

            let report = report_from_server_message(&event);

            if let Some(s) = &senders {
                for persist in persist_events_from_server_message(&event) {
                    let _ = s.persist.send(persist).await;
                }
                for display in display_events_from_server_message(&event) {
                    let _ = s.display.send(display).await;
                }
                for lifecycle in lifecycle_events_from_server_message(&event) {
                    let _ = s.lifecycle.send((*lifecycle).clone());
                }
            }

            if let Some((task_id, report)) = report {
                state
                    .registry
                    .record_task_result(&task_id, report.clone())
                    .await;
                if report.status != "idle" {
                    state.registry.cleanup_runtime(&task_id).await;
                }
                if let Some(tx) = result_tx.take() {
                    let _ = tx.send(report);
                }
                senders = None;
            }
        }
    });
}

fn report_from_server_message(event: &ServerMessage) -> Option<(String, AgentReport)> {
    match event {
        ServerMessage::TaskLifecycle(TaskEvent::Idle {
            task_id,
            summary,
            total_steps,
            ..
        }) => Some((
            task_id.clone(),
            AgentReport {
                text: summary.clone(),
                status: "idle".into(),
                total_steps: *total_steps,
                task_id: None,
            },
        )),
        ServerMessage::TaskLifecycle(TaskEvent::Completed {
            task_id,
            summary,
            final_status,
            total_steps,
            ..
        }) => Some((
            task_id.clone(),
            AgentReport {
                text: summary.clone(),
                status: final_status.clone(),
                total_steps: *total_steps,
                task_id: None,
            },
        )),
        ServerMessage::TaskLifecycle(TaskEvent::Failed { task_id, error, .. }) => Some((
            task_id.clone(),
            AgentReport {
                text: error.clone(),
                status: "error".into(),
                total_steps: 0,
                task_id: None,
            },
        )),
        ServerMessage::TaskLifecycle(TaskEvent::Cancelled { task_id, .. }) => Some((
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
