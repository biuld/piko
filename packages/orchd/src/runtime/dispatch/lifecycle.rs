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
                    if let Some(observer) = &self.task_observer {
                        let _ = observer.send((*task_event).clone());
                    }
                    let _ = lifecycle_tx
                        .send(Arc::new(LifecycleEvent::Task((*task_event).clone())))
                        .await;
                    let _ = persist_tx
                        .send(Arc::new(PersistEvent::TaskEventCommitted(
                            (*task_event).clone(),
                        )))
                        .await;
                }
                LifecycleEvent::Turn(_) => {
                    let _ = lifecycle_tx.send(Arc::new(event)).await;
                }
            }
        }
    }
}
