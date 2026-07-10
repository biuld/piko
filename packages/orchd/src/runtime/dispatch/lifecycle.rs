use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::TaskEvent;
use tokio::sync::mpsc;

use super::{Dispatch, DisplayEvent, LifecycleEvent, PersistEvent};

pub struct LifecycleDispatch {
    name: String,
    events: mpsc::UnboundedReceiver<LifecycleEvent>,
    task_observer: Option<mpsc::UnboundedSender<TaskEvent>>,
}

impl LifecycleDispatch {
    pub fn new(
        session_id: impl Into<String>,
        events: mpsc::UnboundedReceiver<LifecycleEvent>,
    ) -> Self {
        Self {
            name: format!("lifecycle:{}", session_id.into()),
            events,
            task_observer: None,
        }
    }

    pub fn with_task_observer(
        session_id: impl Into<String>,
        events: mpsc::UnboundedReceiver<LifecycleEvent>,
        task_observer: mpsc::UnboundedSender<TaskEvent>,
    ) -> Self {
        Self {
            name: format!("lifecycle:{}", session_id.into()),
            events,
            task_observer: Some(task_observer),
        }
    }
}

#[async_trait]
impl Dispatch for LifecycleDispatch {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(
        &mut self,
        persist_tx: mpsc::Sender<Arc<PersistEvent>>,
        _display_tx: mpsc::Sender<Arc<DisplayEvent>>,
        lifecycle_tx: Option<mpsc::Sender<Arc<LifecycleEvent>>>,
    ) {
        let Some(lifecycle_tx) = lifecycle_tx else {
            return;
        };
        while let Some(event) = self.events.recv().await {
            match &event {
                LifecycleEvent::Task(task_event) => {
                    let task_event = Arc::new(task_event.clone());
                    if let Some(observer) = &self.task_observer
                        && observer.send((*task_event).clone()).is_err()
                    {
                        tracing::error!(
                            task_id = %task_event.task_id(),
                            "task registry lifecycle observer closed"
                        );
                    }
                    if lifecycle_tx
                        .send(Arc::new(LifecycleEvent::Task((*task_event).clone())))
                        .await
                        .is_err()
                    {
                        tracing::error!(task_id = %task_event.task_id(), "lifecycle channel closed");
                    }
                    if persist_tx
                        .send(Arc::new(PersistEvent::TaskEventCommitted(
                            (*task_event).clone(),
                        )))
                        .await
                        .is_err()
                    {
                        tracing::error!(task_id = %task_event.task_id(), "persist channel closed while committing task event");
                    }
                }
                LifecycleEvent::Turn(_) => {
                    if lifecycle_tx.send(Arc::new(event)).await.is_err() {
                        tracing::error!("lifecycle channel closed while routing turn event");
                    }
                }
            }
        }
    }
}
