use std::sync::Arc;

use piko_protocol::TaskEvent;

use crate::application::supervision::registry::TaskRegistry;

/// Internal lifecycle projection from task runtime into `TaskRegistry`.
#[derive(Clone)]
pub(crate) struct InternalLifecycleObserver {
    tx: tokio::sync::mpsc::UnboundedSender<TaskEvent>,
}

impl InternalLifecycleObserver {
    pub(crate) fn new(registry: Arc<TaskRegistry>) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                registry.apply_task_event(&event).await;
            }
        });
        Self { tx }
    }

    pub(crate) fn observe(&self, event: TaskEvent) {
        if self.tx.send(event).is_err() {
            tracing::error!("internal lifecycle observer closed");
        }
    }
}
