use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{Dispatch, DisplayEvent, LifecycleEvent, PersistEvent};

pub struct LifecycleDispatch {
    name: String,
    events: mpsc::UnboundedReceiver<LifecycleEvent>,
}

impl LifecycleDispatch {
    pub fn new(
        session_id: impl Into<String>,
        events: mpsc::UnboundedReceiver<LifecycleEvent>,
    ) -> Self {
        Self {
            name: format!("lifecycle:{}", session_id.into()),
            events,
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

#[derive(Clone)]
pub struct TaskLifecycleDispatcher {
    senders: Option<super::DispatchSenders>,
    host_context: Option<crate::domain::tasks::task::HostTaskContext>,
    task_id: String,
    agent_id: String,
}

impl TaskLifecycleDispatcher {
    pub(crate) fn new(
        senders: Option<super::DispatchSenders>,
        host_context: Option<crate::domain::tasks::task::HostTaskContext>,
        task_id: String,
        agent_id: String,
    ) -> Self {
        Self {
            senders,
            host_context,
            task_id,
            agent_id,
        }
    }

    pub(crate) async fn dispatch(
        &self,
        event: piko_protocol::TaskEvent,
    ) -> Option<crate::domain::events::event::Event> {
        if let Some(ref s) = self.senders {
            let _ = s.lifecycle.send(LifecycleEvent::Task(event));
            None
        } else {
            Some(crate::domain::events::event::Event::TaskLifecycle(event))
        }
    }

    pub(crate) async fn created(
        &self,
        parent_task_id: Option<String>,
        source_agent_id: Option<String>,
        prompt: String,
        turn_id: String,
    ) -> Option<crate::domain::events::event::Event> {
        let hc = self.host_context.as_ref()?;
        let event = piko_protocol::TaskEvent::Created {
            session_id: hc.session_id.clone(),
            turn_id,
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            parent_task_id,
            source_agent_id,
            prompt,
            timestamp: crate::runtime::stream::now_ms(),
        };
        self.dispatch(event).await
    }

    pub(crate) async fn started(&self) -> Option<crate::domain::events::event::Event> {
        let hc = self.host_context.as_ref()?;
        let event = piko_protocol::TaskEvent::Started {
            session_id: hc.session_id.clone(),
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            timestamp: crate::runtime::stream::now_ms(),
        };
        self.dispatch(event).await
    }

    pub(crate) async fn cancelled(&self) -> Option<crate::domain::events::event::Event> {
        let hc = self.host_context.as_ref()?;
        let event = piko_protocol::TaskEvent::Cancelled {
            session_id: hc.session_id.clone(),
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            timestamp: crate::runtime::stream::now_ms(),
        };
        self.dispatch(event).await
    }

    pub(crate) async fn steered(
        &self,
        source_task_id: String,
        source_agent_id: String,
        message: String,
    ) -> Option<crate::domain::events::event::Event> {
        let hc = self.host_context.as_ref()?;
        let event = piko_protocol::TaskEvent::Steered {
            session_id: hc.session_id.clone(),
            task_id: self.task_id.clone(),
            source_task_id,
            source_agent_id,
            message,
            timestamp: crate::runtime::stream::now_ms(),
        };
        self.dispatch(event).await
    }

    pub(crate) async fn idle(
        &self,
        total_steps: u32,
        summary: String,
    ) -> Option<crate::domain::events::event::Event> {
        let hc = self.host_context.as_ref()?;
        let event = piko_protocol::TaskEvent::Idle {
            session_id: hc.session_id.clone(),
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            total_steps,
            summary,
            timestamp: crate::runtime::stream::now_ms(),
        };
        self.dispatch(event).await
    }

    pub(crate) async fn failed(
        &self,
        error: String,
    ) -> Option<crate::domain::events::event::Event> {
        let hc = self.host_context.as_ref()?;
        let event = piko_protocol::TaskEvent::Failed {
            session_id: hc.session_id.clone(),
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            error,
            timestamp: crate::runtime::stream::now_ms(),
        };
        self.dispatch(event).await
    }

    pub(crate) async fn completed(
        &self,
        total_steps: u32,
        summary: String,
    ) -> Option<crate::domain::events::event::Event> {
        let hc = self.host_context.as_ref()?;
        let event = piko_protocol::TaskEvent::Completed {
            session_id: hc.session_id.clone(),
            task_id: self.task_id.clone(),
            agent_id: self.agent_id.clone(),
            total_steps,
            summary,
            final_status: "completed".into(),
            timestamp: crate::runtime::stream::now_ms(),
        };
        self.dispatch(event).await
    }
}
