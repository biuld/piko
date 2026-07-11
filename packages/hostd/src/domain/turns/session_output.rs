use std::path::PathBuf;

use piko_protocol::agent_runtime::{
    RealtimeDelta, RealtimeDeltaEnvelope, SessionEvent, SessionEventEnvelope, TaskSnapshot,
    TaskStatus,
};
use piko_protocol::{DisplayEvent, Message, MessageRole, SessionTreeEntry, TaskEvent};

use crate::api::{MessageEntry, ProtocolError, ServerMessage, ToolCallEntry};
use crate::domain::sessions::HostState;
use crate::infra::storage::TaskRepository;
use crate::protocol::storage_error;

/// Convert a realtime delta envelope into TUI display events.
pub fn display_events_from_delta(envelope: &RealtimeDeltaEnvelope) -> Vec<DisplayEvent> {
    let task_id = envelope.task_id.clone();
    let agent_id = envelope.agent_id.clone();
    let message_id = envelope
        .message_id
        .clone()
        .unwrap_or_else(|| "unknown".into());

    match &envelope.delta {
        RealtimeDelta::MessageStarted { role } => vec![DisplayEvent::MessageStart {
            message_id,
            task_id,
            agent_id,
            role: role.clone(),
        }],
        RealtimeDelta::Text {
            content_index,
            delta,
        } => vec![DisplayEvent::TextDelta {
            task_id,
            agent_id,
            message_id,
            content_index: *content_index,
            delta: delta.clone(),
        }],
        RealtimeDelta::Thinking {
            content_index,
            delta,
        } => vec![DisplayEvent::ThinkingDelta {
            task_id,
            agent_id,
            message_id,
            content_index: *content_index,
            delta: delta.clone(),
        }],
        RealtimeDelta::ToolCall {
            content_index,
            tool_call_id,
            delta,
        } => vec![DisplayEvent::ToolCallDelta {
            task_id,
            agent_id,
            message_id,
            content_index: *content_index,
            tool_call_id: tool_call_id.clone(),
            delta: delta.clone(),
        }],
        RealtimeDelta::MessageEnded {
            stop_reason,
            error_message,
        } => vec![DisplayEvent::MessageEnd {
            message_id,
            task_id,
            agent_id,
            stop_reason: stop_reason.clone(),
            error_message: error_message.clone(),
        }],
    }
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
pub fn apply_message_committed(
    storage: &Option<crate::infra::storage::JsonlSessionRepository>,
    session_path: Option<&PathBuf>,
    state: &mut HostState,
    session_id: &str,
    task_id: &str,
    agent_id: &str,
    message_id: &str,
    work_id: &str,
    role: &MessageRole,
) -> Result<(), ProtocolError> {
    let Some(path) = session_path else {
        return Ok(());
    };

    let repository = TaskRepository::new(path);
    let committed = repository
        .find_committed_message(session_id, task_id, message_id)
        .map_err(storage_error)?
        .ok_or_else(|| {
            ProtocolError::InvalidCommand(format!(
                "committed message {message_id} not found for task {task_id}"
            ))
        })?;

    append_committed_message(
        storage,
        Some(path),
        state,
        session_id,
        task_id,
        agent_id,
        work_id,
        role,
        &committed.message,
        message_id,
        committed.task_seq,
        committed.parent_id.as_deref(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn apply_tool_committed(
    storage: &Option<crate::infra::storage::JsonlSessionRepository>,
    session_path: Option<&PathBuf>,
    state: &mut HostState,
    session_id: &str,
    task_id: &str,
    agent_id: &str,
    message_id: &str,
    work_id: &str,
) -> Result<(), ProtocolError> {
    apply_message_committed(
        storage,
        session_path,
        state,
        session_id,
        task_id,
        agent_id,
        message_id,
        work_id,
        &MessageRole::Tool,
    )
}

#[allow(clippy::too_many_arguments)]
fn append_committed_message(
    _storage: &Option<crate::infra::storage::JsonlSessionRepository>,
    _session_path: Option<&PathBuf>,
    state: &mut HostState,
    session_id: &str,
    task_id: &str,
    agent_id: &str,
    work_id: &str,
    _role: &MessageRole,
    message: &Message,
    message_id: &str,
    task_seq: u64,
    parent_id: Option<&str>,
) -> Result<(), ProtocolError> {
    let parent_id = parent_id.map(str::to_string).or_else(|| {
        state
            .session(session_id)
            .ok()?
            .task_heads
            .get(task_id)
            .cloned()
    });

    let timestamp = message_timestamp(message).to_string();
    let entry = match message {
        Message::ToolCall {
            id,
            name,
            arguments,
            model,
            provider,
            ..
        } => SessionTreeEntry::ToolCall(ToolCallEntry {
            id: message_id.to_string(),
            parent_id,
            timestamp,
            agent_id: Some(agent_id.to_string()),
            task_id: Some(task_id.to_string()),
            tool_call_id: id.clone(),
            tool_name: name.clone(),
            arguments: arguments.clone(),
            parent_message_id: None,
            model: model.clone(),
            provider: provider.clone(),
        }),
        message => SessionTreeEntry::Message(MessageEntry {
            id: message_id.to_string(),
            parent_id,
            timestamp,
            agent_id: agent_id.to_string(),
            task_id: task_id.to_string(),
            work_id: work_id.to_string(),
            task_seq,
            message: message.clone(),
        }),
    };

    // MessageCommitted notifications arrive after PersistSink durability; only
    // project into HostState (and session manifest metadata for non-message entries).
    state.append_task_entry(session_id, task_id, entry)
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
