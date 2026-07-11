//! Unified task event output: session hub, persist barrier, and lifecycle observer.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use piko_protocol::MessageRole;
use piko_protocol::TaskEvent;
use piko_protocol::agent_runtime::{
    RealtimeDelta, RealtimeDeltaEnvelope, SessionEvent, SessionEventEnvelope, TaskSnapshot,
    TaskStatus, WorkSnapshot, WorkStatus,
};

use crate::runtime::events::SharedSessionOutputHub;
use crate::runtime::events::identity::DispatchIdentity;
use crate::runtime::events::internal_lifecycle::InternalLifecycleObserver;
use crate::runtime::persist_sink::SharedPersistSink;
use orchd_api::WorkEventCommit;
use piko_protocol::PersistEvent;

use crate::domain::RealtimeFrame;

use super::persist_commit::commit_persist_event;

/// Per-message realtime delta ordering (resets on `MessageStarted` or message id change).
#[derive(Debug, Default)]
pub(crate) struct DeltaSeqState {
    message_id: Option<String>,
    next_seq: u64,
}

pub(crate) fn allocate_delta_seq(
    state: &Mutex<DeltaSeqState>,
    message_id: &Option<String>,
    delta: &RealtimeDelta,
) -> u64 {
    let mut tracker = state.lock().expect("delta seq lock poisoned");
    if matches!(delta, RealtimeDelta::MessageStarted { .. }) || tracker.message_id != *message_id {
        tracker.message_id = message_id.clone();
        tracker.next_seq = 0;
    }
    let seq = tracker.next_seq;
    tracker.next_seq += 1;
    seq
}

/// Unified task output emitter: persist barrier, session hub, and lifecycle observer.
#[derive(Clone)]
pub(crate) struct TaskEventEmitter {
    pub(crate) identity: DispatchIdentity,
    pub(crate) work_id: String,
    pub(crate) source_turn_id: Option<String>,
    output_hub: SharedSessionOutputHub,
    persist_sink: SharedPersistSink,
    lifecycle_observer: InternalLifecycleObserver,
    head_message_id: Arc<Mutex<Option<String>>>,
    task_seq: Arc<AtomicU64>,
    delta_seq: Arc<Mutex<DeltaSeqState>>,
    persist_error: Arc<Mutex<Option<String>>>,
    persist_commit_lock: Arc<tokio::sync::Mutex<()>>,
}

impl TaskEventEmitter {
    pub(crate) fn new(
        identity: DispatchIdentity,
        work_id: String,
        source_turn_id: Option<String>,
        output_hub: SharedSessionOutputHub,
        persist_sink: SharedPersistSink,
        lifecycle_observer: InternalLifecycleObserver,
        head_message_id: Arc<Mutex<Option<String>>>,
        task_seq: Arc<AtomicU64>,
        delta_seq: Arc<Mutex<DeltaSeqState>>,
        persist_error: Arc<Mutex<Option<String>>>,
        persist_commit_lock: Arc<tokio::sync::Mutex<()>>,
    ) -> Self {
        Self {
            identity,
            work_id,
            source_turn_id,
            output_hub,
            persist_sink,
            lifecycle_observer,
            head_message_id,
            task_seq,
            delta_seq,
            persist_error,
            persist_commit_lock,
        }
    }

    pub(crate) fn take_persist_error(&self) -> Option<String> {
        self.persist_error
            .lock()
            .expect("persist error lock poisoned")
            .take()
    }

    fn record_persist_error(&self, error: impl ToString) {
        *self
            .persist_error
            .lock()
            .expect("persist error lock poisoned") = Some(error.to_string());
    }

    pub(crate) async fn emit_persist_observation(
        &self,
        event: PersistEvent,
        committed_seq: Option<u64>,
    ) {
        self.publish_persist_to_hub(&event, committed_seq).await;
    }

    pub(crate) async fn emit_persist(&self, event: PersistEvent) {
        let event_kind = persist_event_kind(&event);
        let task_seq_before = self.task_seq.load(Ordering::Relaxed);
        let _commit_guard = self.persist_commit_lock.lock().await;
        let sink = match self.persist_sink.resolve().await {
            Ok(sink) => sink,
            Err(error) => {
                tracing::warn!(
                    session_id = %self.identity.session_id(),
                    task_id = %self.identity.task_id(),
                    work_id = %self.work_id,
                    event_kind,
                    %error,
                    "message persist aborted: sink unavailable"
                );
                self.record_persist_error(error);
                return;
            }
        };
        let committed_seq = match commit_persist_event(
            &sink,
            &self.identity,
            &self.work_id,
            &self.head_message_id,
            &self.task_seq,
            &event,
        )
        .await
        {
            Ok(seq) => Some(seq),
            Err(error) => {
                self.record_persist_error(&error);
                tracing::error!(
                    session_id = %self.identity.session_id(),
                    task_id = %self.identity.task_id(),
                    work_id = %self.work_id,
                    event_kind,
                    task_seq = task_seq_before,
                    ?error,
                    "persist sink rejected event"
                );
                return;
            }
        };

        if matches!(event, PersistEvent::Finalized { .. }) {
            tracing::info!(
                session_id = %self.identity.session_id(),
                task_id = %self.identity.task_id(),
                work_id = %self.work_id,
                task_seq = committed_seq.unwrap_or(task_seq_before),
                "assistant message persisted; publishing observation"
            );
        }
        self.publish_persist_to_hub(&event, committed_seq).await;
    }

    pub(crate) async fn emit_realtime(&self, frame: RealtimeFrame) {
        self.publish_realtime_to_hub(&frame).await;
    }

    pub(crate) async fn emit_work_changed(&self, snapshot: WorkSnapshot) {
        let work_id = snapshot.work_id.clone();
        let work_status = snapshot.status.clone();
        let _commit_guard = self.persist_commit_lock.lock().await;
        let sink = match self.persist_sink.resolve().await {
            Ok(sink) => sink,
            Err(error) => {
                tracing::warn!(
                    session_id = %self.identity.session_id(),
                    task_id = %self.identity.task_id(),
                    work_id = %work_id,
                    status = ?work_status,
                    %error,
                    "work lifecycle persist aborted: sink unavailable"
                );
                self.record_persist_error(error);
                return;
            }
        };
        let seq = self.task_seq.load(Ordering::Relaxed) + 1;
        if let Err(error) = sink
            .commit_work_event(WorkEventCommit {
                session_id: self.identity.session_id().clone(),
                task_id: self.identity.task_id().clone(),
                agent_id: self.identity.agent_id().clone(),
                task_seq: seq,
                snapshot: snapshot.clone(),
                committed_at: crate::ports::clock::now_ms(),
            })
            .await
        {
            self.record_persist_error(&error);
            tracing::error!(
                session_id = %self.identity.session_id(),
                task_id = %self.identity.task_id(),
                work_id = %work_id,
                status = ?work_status,
                task_seq = seq,
                ?error,
                "persist sink rejected work lifecycle event"
            );
            return;
        }
        self.task_seq.store(seq, Ordering::Relaxed);
        tracing::info!(
            session_id = %self.identity.session_id(),
            task_id = %self.identity.task_id(),
            work_id = %work_id,
            status = ?work_status,
            task_seq = seq,
            "work lifecycle persisted; publishing to session hub"
        );
        let hub = &self.output_hub;
        let envelope = SessionEventEnvelope {
            task_id: self.identity.task_id().clone(),
            agent_id: self.identity.agent_id().clone(),
            task_seq: seq,
            cursor: hub.cursor(),
            event: SessionEvent::WorkChanged { snapshot },
        };
        if hub.publish_event(envelope).await.is_err() {
            tracing::error!(
                session_id = %self.identity.session_id(),
                task_id = %self.identity.task_id(),
                work_id = %work_id,
                status = ?work_status,
                task_seq = seq,
                "session output hub closed while publishing work lifecycle"
            );
        }
    }

    pub(crate) async fn emit_task_lifecycle(&self, event: TaskEvent) {
        let event_kind = task_lifecycle_kind(&event);
        let task_seq_before = self.task_seq.load(Ordering::Relaxed);
        tracing::info!(
            session_id = %self.identity.session_id(),
            task_id = %event.task_id(),
            work_id = %self.work_id,
            event_kind,
            task_seq = task_seq_before,
            "task lifecycle persist starting"
        );
        let _commit_guard = self.persist_commit_lock.lock().await;
        let sink = match self.persist_sink.resolve().await {
            Ok(sink) => sink,
            Err(error) => {
                tracing::warn!(
                    session_id = %self.identity.session_id(),
                    task_id = %event.task_id(),
                    work_id = %self.work_id,
                    event_kind,
                    %error,
                    "task lifecycle persist aborted: sink unavailable"
                );
                self.record_persist_error(error);
                return;
            }
        };
        let persist_event = PersistEvent::TaskEventCommitted(event.clone());
        if let Err(error) = commit_persist_event(
            &sink,
            &self.identity,
            &self.work_id,
            &self.head_message_id,
            &self.task_seq,
            &persist_event,
        )
        .await
        {
            self.record_persist_error(&error);
            tracing::error!(
                session_id = %self.identity.session_id(),
                task_id = %event.task_id(),
                work_id = %self.work_id,
                event_kind,
                task_seq = task_seq_before,
                ?error,
                "persist sink rejected task lifecycle event"
            );
            return;
        }

        let committed_seq = self.task_seq.load(Ordering::Relaxed);
        tracing::info!(
            session_id = %self.identity.session_id(),
            task_id = %event.task_id(),
            work_id = %self.work_id,
            event_kind,
            task_seq = committed_seq,
            "task lifecycle persisted; publishing to session hub"
        );
        self.publish_task_to_hub(&event).await;
        self.lifecycle_observer.observe(event);
    }

    async fn publish_persist_to_hub(&self, event: &PersistEvent, committed_seq: Option<u64>) {
        let hub = &self.output_hub;
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

    async fn publish_realtime_to_hub(&self, frame: &RealtimeFrame) {
        let hub = &self.output_hub;
        let message_id = Some(frame.message_id.clone());
        let delta_seq = allocate_delta_seq(&self.delta_seq, &message_id, &frame.delta);
        let envelope = RealtimeDeltaEnvelope {
            task_id: frame.task_id.clone(),
            agent_id: frame.agent_id.clone(),
            work_id: self.work_id.clone(),
            message_id,
            delta_seq,
            delta: frame.delta.clone(),
        };
        if hub.publish_delta(envelope).await.is_err() {
            tracing::error!(
                session_id = %self.identity.session_id(),
                "session output hub closed while publishing realtime delta"
            );
        }
    }

    async fn publish_task_to_hub(&self, event: &TaskEvent) {
        let hub = &self.output_hub;
        let Some(snapshot) = task_snapshot_from_event(
            event,
            self.identity.session_id(),
            &self.work_id,
            self.source_turn_id.clone(),
        ) else {
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

fn task_lifecycle_kind(event: &TaskEvent) -> &'static str {
    match event {
        TaskEvent::Created { .. } => "created",
        TaskEvent::Started { .. } => "started",
        TaskEvent::Idle { .. } => "idle",
        TaskEvent::Completed { .. } => "completed",
        TaskEvent::Failed { .. } => "failed",
        TaskEvent::Cancelled { .. } => "cancelled",
        TaskEvent::Closed { .. } => "closed",
        TaskEvent::Reopened { .. } => "reopened",
        TaskEvent::Steered { .. } => "steered",
        TaskEvent::Joined { .. } => "joined",
    }
}

fn persist_event_kind(event: &PersistEvent) -> &'static str {
    match event {
        PersistEvent::UserCommitted { .. } => "user_committed",
        PersistEvent::Finalized { .. } => "assistant_finalized",
        PersistEvent::ToolCallCommitted { .. } => "tool_call_committed",
        PersistEvent::ToolResultCommitted { .. } => "tool_result_committed",
        PersistEvent::TaskEventCommitted(task_event) => task_lifecycle_kind(task_event),
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
        PersistEvent::ToolCallCommitted {
            message_id,
            work_id,
            ..
        } => Some(SessionEvent::MessageCommitted {
            message_id: message_id.clone(),
            work_id: work_id.clone(),
            role: MessageRole::Tool,
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

fn task_snapshot_from_event(
    event: &TaskEvent,
    session_id: &str,
    work_id: &str,
    source_turn_id: Option<String>,
) -> Option<TaskSnapshot> {
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
                work_id: work_id.to_string(),
                status: WorkStatus::Running,
                source_turn_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::MessageRole;

    #[test]
    fn committed_tool_call_is_published_as_reliable_message() {
        let event = PersistEvent::ToolCallCommitted {
            session_id: "session-1".into(),
            message_id: "message-tool".into(),
            task_id: "task-1".into(),
            agent_id: "main".into(),
            work_id: "work-1".into(),
            parent_message_id: "assistant-1".into(),
            message: piko_protocol::Message::ToolCall {
                id: "call-1".into(),
                name: "read".into(),
                arguments: serde_json::json!({}),
                model: None,
                provider: None,
                timestamp: None,
            },
        };

        assert!(matches!(
            session_event_from_persist(&event),
            Some(SessionEvent::MessageCommitted {
                message_id,
                role: MessageRole::Tool,
                ..
            }) if message_id == "message-tool"
        ));
    }

    #[test]
    fn delta_seq_increments_within_message_and_resets_on_message_started() {
        let state = Mutex::new(DeltaSeqState::default());
        let message_a = Some("msg-a".to_string());
        let message_b = Some("msg-b".to_string());

        assert_eq!(
            allocate_delta_seq(
                &state,
                &message_a,
                &RealtimeDelta::MessageStarted {
                    role: MessageRole::Assistant,
                },
            ),
            0
        );
        assert_eq!(
            allocate_delta_seq(
                &state,
                &message_a,
                &RealtimeDelta::Text {
                    content_index: 0,
                    delta: "hi".into(),
                },
            ),
            1
        );
        assert_eq!(
            allocate_delta_seq(
                &state,
                &message_a,
                &RealtimeDelta::MessageEnded {
                    stop_reason: None,
                    error_message: None,
                },
            ),
            2
        );
        assert_eq!(
            allocate_delta_seq(
                &state,
                &message_b,
                &RealtimeDelta::MessageStarted {
                    role: MessageRole::Assistant,
                },
            ),
            0
        );
        assert_eq!(
            allocate_delta_seq(
                &state,
                &message_b,
                &RealtimeDelta::Text {
                    content_index: 0,
                    delta: "next".into(),
                },
            ),
            1
        );
    }

    #[test]
    fn delta_seq_resets_when_message_id_changes_without_message_started() {
        let state = Mutex::new(DeltaSeqState::default());
        let message_a = Some("msg-a".to_string());
        let message_b = Some("msg-b".to_string());

        assert_eq!(
            allocate_delta_seq(
                &state,
                &message_a,
                &RealtimeDelta::Text {
                    content_index: 0,
                    delta: "a".into(),
                },
            ),
            0
        );
        assert_eq!(
            allocate_delta_seq(
                &state,
                &message_b,
                &RealtimeDelta::Text {
                    content_index: 0,
                    delta: "b".into(),
                },
            ),
            0
        );
    }
}
