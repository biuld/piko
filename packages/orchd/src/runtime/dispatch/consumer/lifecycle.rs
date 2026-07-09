use std::sync::{Arc, Mutex};

use crate::domain::events::event::Event;
use crate::runtime::dispatch::consumer::DispatchIdentity;
use crate::runtime::dispatch::{DispatchSenders, LifecycleEvent, PersistEvent};
use crate::runtime::utils::now_ms;
use piko_protocol::TaskEvent;

#[derive(Clone, Default)]
struct SharedLifecycleEventCollector(Arc<Mutex<Vec<Event>>>);

impl SharedLifecycleEventCollector {
    fn take(&self) -> Vec<Event> {
        let mut events = self.0.lock().expect("lifecycle event collector poisoned");
        std::mem::take(&mut *events)
    }

    fn push(&self, event: Event) {
        self.0
            .lock()
            .expect("lifecycle event collector poisoned")
            .push(event);
    }
}

pub(crate) struct TaskLifecycleConsumer {
    senders: Option<DispatchSenders>,
    identity: DispatchIdentity,
    turn_id: String,
    collector: SharedLifecycleEventCollector,
}

impl TaskLifecycleConsumer {
    pub(crate) fn new(
        senders: Option<DispatchSenders>,
        identity: DispatchIdentity,
        turn_id: String,
    ) -> Self {
        Self {
            senders,
            identity,
            turn_id,
            collector: SharedLifecycleEventCollector::default(),
        }
    }

    pub(crate) fn take_events(&self) -> Vec<Event> {
        self.collector.take()
    }

    async fn emit(&self, event: TaskEvent) {
        if let Some(ref senders) = self.senders {
            let _ = senders.lifecycle.send(LifecycleEvent::Task(event));
        } else {
            self.collector.push(Event::TaskLifecycle(event.clone()));
            self.collector
                .push(Event::Persist(PersistEvent::TaskEventCommitted(event)));
        }
    }

    async fn emit_from_context<F>(&self, build: F)
    where
        F: FnOnce(&str, &str, &str, &str) -> TaskEvent,
    {
        self.emit(build(
            self.identity.session_id(),
            &self.turn_id,
            self.identity.task_id(),
            self.identity.agent_id(),
        ))
        .await;
    }

    pub(crate) async fn on_task_created(
        &self,
        parent_task_id: Option<&str>,
        source_agent_id: Option<&str>,
        prompt: &str,
        turn_id: &str,
    ) {
        self.emit_from_context(|session_id, _runtime_turn_id, task_id, agent_id| {
            TaskEvent::Created {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                parent_task_id: parent_task_id.map(str::to_string),
                source_agent_id: source_agent_id.map(str::to_string),
                prompt: prompt.to_string(),
                timestamp: now_ms(),
            }
        })
        .await;
    }

    pub(crate) async fn on_task_started(&self) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Started {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_steered(
        &self,
        source_task_id: &str,
        source_agent_id: &str,
        message: &str,
    ) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, _agent_id| TaskEvent::Steered {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                source_task_id: source_task_id.to_string(),
                source_agent_id: source_agent_id.to_string(),
                message: message.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_idle(&self, total_steps: u32, summary: &str) {
        self.emit_from_context(|session_id, _turn_id, task_id, agent_id| TaskEvent::Idle {
            session_id: session_id.to_string(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            total_steps,
            summary: summary.to_string(),
            timestamp: now_ms(),
        })
        .await;
    }

    pub(crate) async fn on_task_failed(&self, error: &str) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Failed {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                error: error.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_completed(&self, total_steps: u32, summary: &str) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Completed {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                total_steps,
                summary: summary.to_string(),
                final_status: "completed".into(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_cancelled(&self) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Cancelled {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_closed(&self) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Closed {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    pub(crate) async fn on_task_reopened(&self) {
        self.emit_from_context(
            |session_id, _turn_id, task_id, agent_id| TaskEvent::Reopened {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
                agent_id: agent_id.to_string(),
                timestamp: now_ms(),
            },
        )
        .await;
    }
}
