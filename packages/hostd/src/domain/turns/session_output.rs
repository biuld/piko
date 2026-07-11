use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use orchd_api::{
    MessageCommit, PersistAck, PersistError, PersistSink, TaskEventCommit, WorkEventCommit,
};
use piko_protocol::agent_runtime::{
    RealtimeDeltaEnvelope, SessionEvent, SessionEventEnvelope, TaskSnapshot, TaskStatus,
};
use piko_protocol::{
    Message, RealtimeMessageEvent, SessionTreeEntry, TaskEvent, TranscriptCommittedEvent,
};

use crate::api::{MessageEntry, ProtocolError, ServerMessage};
use crate::domain::sessions::HostState;
use crate::infra::storage::TaskRepository;

/// Session-scoped durable write adapter (barrier path). One instance per open session.
pub struct SessionPersistSink {
    repository: TaskRepository,
    state: Arc<tokio::sync::Mutex<HostState>>,
}

/// Backward-compatible alias while call sites migrate.
pub type ProjectingPersistSink = SessionPersistSink;

impl SessionPersistSink {
    pub fn new(
        session_path: impl Into<PathBuf>,
        state: Arc<tokio::sync::Mutex<HostState>>,
    ) -> Self {
        Self {
            repository: TaskRepository::new(session_path),
            state,
        }
    }
}

/// Observation path: project a committed message for TUI emission.
///
/// Prefer in-memory HostState updated by the session-scoped barrier sink, then
/// fall back to durable storage. The barrier always completes before hub
/// `MessageCommitted` notifications are published in-process; HostState avoids
/// read races while the task shard is still being appended.
pub fn project_committed_message(
    state: &HostState,
    repository: Option<&TaskRepository>,
    session_id: &str,
    task_id: &str,
    message_id: &str,
) -> Option<TranscriptCommittedEvent> {
    if let Some(projected) =
        project_committed_message_from_state(state, session_id, task_id, message_id)
    {
        return Some(projected);
    }
    repository.and_then(|repository| {
        project_committed_message_from_repository(repository, session_id, task_id, message_id)
    })
}

fn project_committed_message_from_state(
    state: &HostState,
    session_id: &str,
    task_id: &str,
    message_id: &str,
) -> Option<TranscriptCommittedEvent> {
    let session = state.session(session_id).ok()?;
    let SessionTreeEntry::Message(message) = session
        .entries
        .iter()
        .find(|entry| entry.id() == message_id)?
    else {
        return None;
    };
    if message.task_id != task_id {
        return None;
    }
    Some(TranscriptCommittedEvent {
        session_id: session_id.to_string(),
        task_id: task_id.to_string(),
        agent_id: message.agent_id.clone(),
        work_id: message.work_id.clone(),
        message_id: message_id.to_string(),
        task_seq: message.task_seq,
        message: message.message.clone(),
    })
}

fn project_committed_message_from_repository(
    repository: &TaskRepository,
    session_id: &str,
    task_id: &str,
    message_id: &str,
) -> Option<TranscriptCommittedEvent> {
    let message = repository
        .find_committed_message_lenient(task_id, message_id)
        .or_else(|| {
            repository
                .find_committed_message(session_id, task_id, message_id)
                .ok()
                .flatten()
        })?;
    Some(TranscriptCommittedEvent {
        session_id: session_id.to_string(),
        task_id: task_id.to_string(),
        agent_id: message.agent_id,
        work_id: message.work_id,
        message_id: message_id.to_string(),
        task_seq: message.task_seq,
        message: message.message,
    })
}

#[async_trait]
impl PersistSink for SessionPersistSink {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError> {
        let ack = self.repository.commit_message(event.clone())?;
        {
            let mut state = self.state.lock().await;
            append_committed_message(
                &mut state,
                &event.session_id,
                &event.task_id,
                &event.agent_id,
                &event.work_id,
                &event.message,
                &event.message_id,
                event.task_seq,
                event.parent_message_id.as_deref(),
            )
            .map_err(|error| PersistError::Failed(error.to_string()))?;
        }
        Ok(ack)
    }

    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError> {
        let ack = self.repository.commit_task_event(event.clone())?;
        if let TaskEvent::Created {
            session_id,
            task_id,
            parent_task_id,
            ..
        } = event.event
            && parent_task_id.is_none()
            && let Ok(session) = self.state.lock().await.session_mut(&session_id)
        {
            session.active_task_id = Some(task_id);
        }
        Ok(ack)
    }

    async fn commit_work_event(&self, event: WorkEventCommit) -> Result<PersistAck, PersistError> {
        self.repository.commit_work_event(event)
    }
}

/// Convert a best-effort orchd delta into the hostd-to-client realtime projection.
pub fn realtime_message_from_delta(
    session_id: &str,
    envelope: &RealtimeDeltaEnvelope,
) -> Option<RealtimeMessageEvent> {
    let task_id = envelope.task_id.clone();
    let agent_id = envelope.agent_id.clone();
    let message_id = envelope.message_id.clone()?;

    Some(RealtimeMessageEvent {
        session_id: session_id.to_string(),
        task_id,
        agent_id,
        message_id,
        delta_seq: envelope.delta_seq,
        delta: envelope.delta.clone(),
    })
}

/// Project a durable runtime snapshot into the host/TUI lifecycle wire format.
pub fn task_event_from_snapshot(
    snapshot: &TaskSnapshot,
    turn_id: &str,
    timestamp: i64,
) -> Option<TaskEvent> {
    let session_id = snapshot.session_id.clone();
    let task_id = snapshot.task_id.clone();
    let agent_id = snapshot.agent_id.clone();
    let parent_task_id = snapshot.parent_task_id.clone();

    Some(match snapshot.status {
        TaskStatus::Created => TaskEvent::Created {
            session_id,
            task_id,
            agent_id,
            parent_task_id,
            source_agent_id: None,
            prompt: String::new(),
            work_id: turn_id.to_string(),
            timestamp,
        },
        TaskStatus::Running => TaskEvent::Started {
            session_id,
            task_id,
            agent_id,
            timestamp,
        },
        TaskStatus::Idle => TaskEvent::Idle {
            session_id,
            task_id,
            agent_id,
            total_steps: 0,
            summary: String::new(),
            timestamp,
        },
        TaskStatus::Terminated => TaskEvent::Completed {
            session_id,
            task_id,
            agent_id,
            total_steps: 0,
            summary: String::new(),
            final_status: "completed".into(),
            timestamp,
        },
        TaskStatus::Failed => TaskEvent::Failed {
            session_id,
            task_id,
            agent_id,
            error: String::new(),
            timestamp,
        },
        TaskStatus::Closed => TaskEvent::Closed {
            session_id,
            task_id,
            agent_id,
            timestamp,
        },
    })
}

/// Convert a durable `TaskChanged` notification into a lifecycle server message.
pub fn task_lifecycle_from_task_changed(
    envelope: &SessionEventEnvelope,
    turn_id: &str,
    timestamp: i64,
) -> Option<ServerMessage> {
    let SessionEvent::TaskChanged { snapshot } = &envelope.event else {
        return None;
    };
    task_event_from_snapshot(snapshot, turn_id, timestamp).map(ServerMessage::TaskLifecycle)
}

pub fn is_root_task_terminal(snapshot: &TaskSnapshot, root_task_id: &str) -> bool {
    snapshot.task_id == root_task_id
        && matches!(
            snapshot.status,
            TaskStatus::Idle | TaskStatus::Terminated | TaskStatus::Failed
        )
}

#[allow(clippy::too_many_arguments)]
fn append_committed_message(
    state: &mut HostState,
    session_id: &str,
    task_id: &str,
    agent_id: &str,
    work_id: &str,
    message: &Message,
    message_id: &str,
    task_seq: u64,
    parent_id: Option<&str>,
) -> Result<Option<TranscriptCommittedEvent>, ProtocolError> {
    let is_new = state
        .session(session_id)?
        .entries
        .iter()
        .all(|entry| entry.id() != message_id);
    if is_new
        && let Message::Assistant {
            usage: Some(usage), ..
        } = message
        && let Ok(session) = state.session_mut(session_id)
    {
        session.accumulate_usage(usage);
    }
    let parent_id = parent_id.map(str::to_string).or_else(|| {
        state
            .session(session_id)
            .ok()?
            .task_heads
            .get(task_id)
            .cloned()
    });

    let timestamp = message_timestamp(message).to_string();
    let entry = SessionTreeEntry::Message(MessageEntry {
        id: message_id.to_string(),
        parent_id,
        timestamp,
        agent_id: agent_id.to_string(),
        task_id: task_id.to_string(),
        work_id: work_id.to_string(),
        task_seq,
        message: message.clone(),
    });

    // MessageCommitted notifications arrive after PersistSink durability; only
    // project into HostState (and session manifest metadata for non-message entries).
    state.append_task_entry(session_id, task_id, entry)?;
    Ok(Some(TranscriptCommittedEvent {
        session_id: session_id.to_string(),
        task_id: task_id.to_string(),
        agent_id: agent_id.to_string(),
        work_id: work_id.to_string(),
        message_id: message_id.to_string(),
        task_seq,
        message: message.clone(),
    }))
}

fn message_timestamp(message: &Message) -> &i64 {
    const DEFAULT: i64 = 0;
    match message {
        Message::User { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::Assistant { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::ToolCall { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::ToolResult { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::agent_runtime::RealtimeDelta;

    #[test]
    fn realtime_projection_preserves_message_identity_and_delta_seq() {
        let event = realtime_message_from_delta(
            "session-1",
            &RealtimeDeltaEnvelope {
                task_id: "task-1".into(),
                agent_id: "main".into(),
                work_id: "work-1".into(),
                message_id: Some("message-1".into()),
                delta_seq: 7,
                delta: RealtimeDelta::Text {
                    content_index: 0,
                    delta: "hello".into(),
                },
            },
        )
        .unwrap();

        assert_eq!(event.session_id, "session-1");
        assert_eq!(event.message_id, "message-1");
        assert_eq!(event.delta_seq, 7);
        assert!(matches!(
            event.delta,
            RealtimeDelta::Text { delta, .. } if delta == "hello"
        ));
    }

    #[test]
    fn realtime_projection_rejects_missing_message_identity() {
        assert!(
            realtime_message_from_delta(
                "session-1",
                &RealtimeDeltaEnvelope {
                    task_id: "task-1".into(),
                    agent_id: "main".into(),
                    work_id: "work-1".into(),
                    message_id: None,
                    delta_seq: 0,
                    delta: RealtimeDelta::MessageStarted {
                        role: piko_protocol::MessageRole::Assistant,
                    },
                },
            )
            .is_none()
        );
    }

    #[test]
    fn project_committed_message_reads_task_repository() {
        use orchd_api::{MessageCommit, TaskEventCommit};
        use piko_protocol::MessageContent;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let repository =
            TaskRepository::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
                .unwrap();
        repository
            .commit_task_event(TaskEventCommit {
                session_id: "session-1".into(),
                task_id: "task-1".into(),
                agent_id: "main".into(),
                task_seq: 1,
                event: TaskEvent::Created {
                    session_id: "session-1".into(),
                    work_id: "work-1".into(),
                    task_id: "task-1".into(),
                    agent_id: "main".into(),
                    parent_task_id: None,
                    source_agent_id: None,
                    prompt: String::new(),
                    timestamp: 1,
                },
                committed_at: 1,
            })
            .unwrap();
        repository
            .commit_message(MessageCommit {
                session_id: "session-1".into(),
                task_id: "task-1".into(),
                agent_id: "main".into(),
                work_id: "work-1".into(),
                task_seq: 2,
                message_id: "msg-followup".into(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String("second turn".into()),
                    timestamp: Some(2),
                },
                committed_at: 2,
            })
            .unwrap();

        let state = HostState::default();
        let projection = project_committed_message(
            &state,
            Some(&repository),
            "session-1",
            "task-1",
            "msg-followup",
        )
        .expect("projection should load from repository");
        assert_eq!(projection.message_id, "msg-followup");
        assert_eq!(projection.task_seq, 2);
    }

    #[test]
    fn project_committed_message_prefers_barrier_state() {
        use orchd_api::{MessageCommit, TaskEventCommit};
        use piko_protocol::MessageContent;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let repository =
            TaskRepository::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
                .unwrap();
        repository
            .commit_task_event(TaskEventCommit {
                session_id: "session-1".into(),
                task_id: "task-1".into(),
                agent_id: "main".into(),
                task_seq: 1,
                event: TaskEvent::Created {
                    session_id: "session-1".into(),
                    work_id: "work-1".into(),
                    task_id: "task-1".into(),
                    agent_id: "main".into(),
                    parent_task_id: None,
                    source_agent_id: None,
                    prompt: String::new(),
                    timestamp: 1,
                },
                committed_at: 1,
            })
            .unwrap();

        let state = Arc::new(tokio::sync::Mutex::new(HostState::default()));
        {
            let mut host_state = state.blocking_lock();
            host_state.insert_session(crate::domain::sessions::SessionState::new(
                "session-1".into(),
                "/project".into(),
            ));
        }
        let sink = SessionPersistSink::new(temp.path(), Arc::clone(&state));
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            sink.commit_message(MessageCommit {
                session_id: "session-1".into(),
                task_id: "task-1".into(),
                agent_id: "main".into(),
                work_id: "work-2".into(),
                task_seq: 2,
                message_id: "msg-followup".into(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String("second turn".into()),
                    timestamp: Some(2),
                },
                committed_at: 2,
            })
            .await
            .unwrap();
        });

        let host_state = state.blocking_lock();
        let projection =
            project_committed_message(&host_state, None, "session-1", "task-1", "msg-followup")
                .expect("projection should load from barrier-updated host state");
        assert_eq!(projection.message_id, "msg-followup");
        assert_eq!(projection.task_seq, 2);
    }
}
