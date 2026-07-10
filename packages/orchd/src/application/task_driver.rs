use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;
use futures_util::StreamExt;

use crate::runtime::dispatch::DispatchSenders;
use piko_protocol::{ServerMessage, TaskEvent};

use super::supervisor::SupervisorState;

pub(crate) fn spawn_task_driver(
    state: Arc<SupervisorState>,
    task_id: String,
    mut stream: Pin<Box<dyn Stream<Item = ServerMessage> + Send>>,
    senders: Option<DispatchSenders>,
) {
    tokio::spawn(async move {
        use crate::runtime::dispatch::{
            display_events_from_server_message, lifecycle_events_from_server_message,
            persist_events_from_server_message,
        };

        let mut senders = senders;

        while let Some(event) = stream.next().await {
            if let ServerMessage::TaskLifecycle(task_event) = &event
                && state.task_event_tx.send(task_event.clone()).is_err()
            {
                tracing::error!(
                    task_id = %task_event.task_id(),
                    "supervisor task event input closed"
                );
            }

            let work_finished = matches!(
                event,
                ServerMessage::TaskLifecycle(
                    TaskEvent::Idle { .. }
                        | TaskEvent::Completed { .. }
                        | TaskEvent::Failed { .. }
                        | TaskEvent::Cancelled { .. }
                )
            );

            if let Some(s) = &senders {
                for persist in persist_events_from_server_message(&event) {
                    if s.persist.send(persist).await.is_err() {
                        tracing::error!("persist channel closed while forwarding task event");
                    }
                }
                for display in display_events_from_server_message(&event) {
                    if s.display.send(display).await.is_err() {
                        tracing::error!("display channel closed while forwarding task event");
                    }
                }
                for lifecycle in lifecycle_events_from_server_message(&event) {
                    if s.lifecycle.send((*lifecycle).clone()).is_err() {
                        tracing::error!(
                            "lifecycle dispatch input closed while forwarding task event"
                        );
                    }
                }
            }

            if work_finished {
                senders = None;
            }
        }

        state.registry.cleanup_runtime(&task_id).await;
    });
}
