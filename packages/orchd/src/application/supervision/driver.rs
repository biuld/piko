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
            if let ServerMessage::TaskLifecycle(task_event) = &event {
                state.lifecycle_observer.observe(task_event.clone());
            }
        }

        state.registry.cleanup_runtime(&task_id).await;
    });
}
