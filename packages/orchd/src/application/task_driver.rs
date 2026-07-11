use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;
use futures_util::StreamExt;

use piko_protocol::ServerMessage;

use super::supervisor::SupervisorState;

pub(crate) fn spawn_task_driver(
    state: Arc<SupervisorState>,
    task_id: String,
    mut stream: Pin<Box<dyn Stream<Item = ServerMessage> + Send>>,
) {
    tokio::spawn(async move {
        while let Some(event) = stream.next().await {
            if let ServerMessage::TaskLifecycle(task_event) = &event
                && state.task_event_tx.send(task_event.clone()).is_err()
            {
                tracing::error!(
                    task_id = %task_event.task_id(),
                    "supervisor task event input closed"
                );
            }
        }

        state.registry.cleanup_runtime(&task_id).await;
    });
}
