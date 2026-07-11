//! Unified task event output: session hub, persist barrier, and local stream collector.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use piko_protocol::MessageRole;
use piko_protocol::TaskEvent;
use piko_protocol::agent_runtime::{
    RealtimeDelta, RealtimeDeltaEnvelope, SessionEvent, SessionEventEnvelope, TaskSnapshot,
    TaskStatus, WorkSnapshot, WorkStatus,
};

use crate::domain::events::event::Event;
use crate::integration::PersistSink;
use crate::runtime::events::SharedSessionOutputHub;
use crate::runtime::events::identity::DispatchIdentity;
use piko_protocol::{DisplayEvent, PersistEvent};

use super::persist_commit::commit_persist_event;

#[derive(Clone, Default)]
struct LocalEventCollector(Arc<Mutex<Vec<Event>>>);

impl LocalEventCollector {
    fn take(&self) -> Vec<Event> {
        let mut events = self.0.lock().expect("local event collector poisoned");
        std::mem::take(&mut *events)
    }

    fn push(&self, event: Event) {
        self.0
            .lock()
            .expect("local event collector poisoned")
            .push(event);
    }
}

/// Unified task output emitter: persist barrier, session hub, and local collector.
#[derive(Clone)]
pub(crate) struct TaskEventEmitter {
    pub(crate) identity: DispatchIdentity,
    pub(crate) turn_id: String,
    output_hub: Option<SharedSessionOutputHub>,
    persist_sink: Option<Arc<dyn PersistSink>>,
    head_message_id: Arc<Mutex<Option<String>>>,
    task_seq: Arc<AtomicU64>,
    local: LocalEventCollector,
}

impl TaskEventEmitter {
    pub(crate) fn new(
        identity: DispatchIdentity,
        turn_id: String,
        output_hub: Option<SharedSessionOutputHub>,
        persist_sink: Option<Arc<dyn PersistSink>>,
        head_message_id: Arc<Mutex<Option<String>>>,
        task_seq: Arc<AtomicU64>,
    ) -> Self {
        Self {
            identity,
            turn_id,
            output_hub,
            persist_sink,
            head_message_id,
            task_seq,
            local: LocalEventCollector::default(),
        }
    }

    pub(crate) fn take_local_events(&self) -> Vec<Event> {
        self.local.take()
    }

    pub(crate) async fn emit_persist_observation(
        &self,
        event: PersistEvent,
        committed_seq: Option<u64>,
    ) {
        self.publish_persist_to_hub(&event, committed_seq).await;
        self.local.push(Event::Persist(event));
    }

    pub(crate) async fn emit_persist(&self, event: PersistEvent) {
        let mut committed_seq = None;
        if let Some(sink) = &self.persist_sink {
            match commit_persist_event(
                sink,
                &self.identity,
                &self.turn_id,
                &self.head_message_id,
                &self.task_seq,
                &event,
            )
            .await
            {
                Ok(seq) => committed_seq = Some(seq),
                Err(error) => {
                    tracing::error!(
                        task_id = %self.identity.task_id(),
                        ?error,
                        "persist sink rejected event"
                    );
                    return;
                }
            }
        }

        self.publish_persist_to_hub(&event, committed_seq).await;
        self.local.push(Event::Persist(event));
    }

    pub(crate) async fn emit_display(&self, event: DisplayEvent) {
        self.publish_display_to_hub(&event).await;
        self.local.push(Event::Display(event));
    }

    pub(crate) async fn emit_task_lifecycle(&self, event: TaskEvent) {
        if let Some(sink) = &self.persist_sink {
            let persist_event = PersistEvent::TaskEventCommitted(event.clone());
            if commit_persist_event(
                sink,
                &self.identity,
                &self.turn_id,
                &self.head_message_id,
                &self.task_seq,
                &persist_event,
            )
            .await
            .is_err()
            {
                tracing::error!(
                    task_id = %event.task_id(),
                    "persist sink rejected task lifecycle event"
                );
                return;
            }
        }

        self.publish_task_to_hub(&event).await;
        self.local.push(Event::TaskLifecycle(event.clone()));
        self.local
            .push(Event::Persist(PersistEvent::TaskEventCommitted(event)));
    }

    async fn publish_persist_to_hub(&self, event: &PersistEvent, committed_seq: Option<u64>) {
        let Some(hub) = &self.output_hub else {
            return;
        };
        let Some(session_event) = session_event_from_persist(event) else {
            return;
        };
        let task_seq = committed_seq.unwrap_or_else(|| self.task_seq.load(Ordering::Relaxed));
        let envelope = SessionEventEnvelope {
            task_id: self.identity.task_id().clone(),
            agent_id: self.identity.agent_id().clone(),
            task_seq,
            cursor: hub.cursor(),
            event: session_event,
        };
        if hub.publish_event(envelope).await.is_err() {
            tracing::error!(
                session_id = %self.identity.session_id(),
                "session output hub closed while publishing persist event"
            );
        }
    }

    async fn publish_display_to_hub(&self, event: &DisplayEvent) {
        let Some(hub) = &self.output_hub else {
            return;
        };
        let Some(delta) = realtime_delta_from_display(event) else {
            return;
        };
        let envelope = RealtimeDeltaEnvelope {
            task_id: self.identity.task_id().clone(),
            agent_id: self.identity.agent_id().clone(),
            work_id: self.turn_id.clone(),
            message_id: display_message_id(event),
            delta_seq: 0,
            delta,
        };
        if hub.publish_delta(envelope).await.is_err() {
            tracing::error!(
                session_id = %self.identity.session_id(),
                "session output hub closed while publishing display delta"
            );
        }
    }

    async fn publish_task_to_hub(&self, event: &TaskEvent) {
        let Some(hub) = &self.output_hub else {
            return;
        };
        let Some(snapshot) = task_snapshot_from_event(event, self.identity.session_id()) else {
            return;
        };
        let envelope = SessionEventEnvelope {
            task_id: self.identity.task_id().clone(),
            agent_id: self.identity.agent_id().clone(),
            task_seq: self.task_seq.load(Ordering::Relaxed),
            cursor: hub.cursor(),
            event: SessionEvent::TaskChanged { snapshot },
        };
        if hub.publish_event(envelope).await.is_err() {
            tracing::error!(
                session_id = %self.identity.session_id(),
                "session output hub closed while publishing task lifecycle"
            );
        }
    }
}

fn session_event_from_persist(event: &PersistEvent) -> Option<SessionEvent> {
    match event {
        PersistEvent::UserCommitted {
            message_id,
            work_id,
            ..
        } => Some(SessionEvent::MessageCommitted {
            message_id: message_id.clone(),
            work_id: work_id.clone(),
            role: MessageRole::User,
        }),
        PersistEvent::Finalized {
            message_id,
            work_id,
            ..
        } => Some(SessionEvent::MessageCommitted {
            message_id: message_id.clone(),
            work_id: work_id.clone(),
            role: MessageRole::Assistant,
        }),
        PersistEvent::ToolResultCommitted {
            message_id,
            work_id,
            message,
            ..
        } => {
            let tool_call_id = match message {
                piko_protocol::Message::ToolResult { tool_call_id, .. } => tool_call_id.clone(),
                _ => message_id.clone(),
            };
            Some(SessionEvent::ToolCommitted {
                message_id: message_id.clone(),
                work_id: work_id.clone(),
                tool_call_id,
            })
        }
        _ => None,
    }
}

fn display_message_id(event: &DisplayEvent) -> Option<String> {
    match event {
        DisplayEvent::TextDelta { message_id, .. }
        | DisplayEvent::ThinkingDelta { message_id, .. }
        | DisplayEvent::ToolCallDelta { message_id, .. }
        | DisplayEvent::MessageStart { message_id, .. }
        | DisplayEvent::MessageEnd { message_id, .. }
        | DisplayEvent::Finalized { message_id, .. } => Some(message_id.clone()),
        _ => None,
    }
}

fn realtime_delta_from_display(event: &DisplayEvent) -> Option<RealtimeDelta> {
    match event {
        DisplayEvent::TextDelta {
            content_index,
            delta,
            ..
        } => Some(RealtimeDelta::Text {
            content_index: *content_index,
            delta: delta.clone(),
        }),
        DisplayEvent::ThinkingDelta {
            content_index,
            delta,
            ..
        } => Some(RealtimeDelta::Thinking {
            content_index: *content_index,
            delta: delta.clone(),
        }),
        DisplayEvent::ToolCallDelta {
            content_index,
            tool_call_id,
            delta,
            ..
        } => Some(RealtimeDelta::ToolCall {
            content_index: *content_index,
            tool_call_id: tool_call_id.clone(),
            delta: delta.clone(),
        }),
        DisplayEvent::MessageStart { role, .. } => {
            Some(RealtimeDelta::MessageStarted { role: role.clone() })
        }
        DisplayEvent::MessageEnd {
            stop_reason,
            error_message,
            ..
        } => Some(RealtimeDelta::MessageEnded {
            stop_reason: stop_reason.clone(),
            error_message: error_message.clone(),
        }),
        _ => None,
    }
}

fn task_snapshot_from_event(event: &TaskEvent, session_id: &str) -> Option<TaskSnapshot> {
    let (task_id, agent_id, parent_task_id, status, active_work) = match event {
        TaskEvent::Created {
            task_id,
            agent_id,
            parent_task_id,
            ..
        } => (
            task_id.clone(),
            agent_id.clone(),
            parent_task_id.clone(),
            TaskStatus::Created,
            None,
        ),
        TaskEvent::Started {
            task_id, agent_id, ..
        } => (
            task_id.clone(),
            agent_id.clone(),
            None,
            TaskStatus::Running,
            Some(WorkSnapshot {
                work_id: String::new(),
                status: WorkStatus::Running,
            }),
        ),
        TaskEvent::Idle {
            task_id, agent_id, ..
        } => (
            task_id.clone(),
            agent_id.clone(),
            None,
            TaskStatus::Idle,
            None,
        ),
        TaskEvent::Completed {
            task_id, agent_id, ..
        } => (
            task_id.clone(),
            agent_id.clone(),
            None,
            TaskStatus::Terminated,
            None,
        ),
        TaskEvent::Failed {
            task_id, agent_id, ..
        } => (
            task_id.clone(),
            agent_id.clone(),
            None,
            TaskStatus::Failed,
            None,
        ),
        TaskEvent::Closed {
            task_id, agent_id, ..
        } => (
            task_id.clone(),
            agent_id.clone(),
            None,
            TaskStatus::Closed,
            None,
        ),
        TaskEvent::Reopened {
            task_id, agent_id, ..
        } => (
            task_id.clone(),
            agent_id.clone(),
            None,
            TaskStatus::Idle,
            None,
        ),
        TaskEvent::Cancelled {
            task_id, agent_id, ..
        } => (
            task_id.clone(),
            agent_id.clone(),
            None,
            TaskStatus::Terminated,
            None,
        ),
        _ => return None,
    };

    Some(TaskSnapshot {
        session_id: session_id.to_string(),
        task_id,
        agent_id,
        parent_task_id,
        status,
        active_work,
    })
}
