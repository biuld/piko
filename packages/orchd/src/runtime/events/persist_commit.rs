use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use piko_protocol::Message;

use crate::integration::{MessageCommit, PersistError, PersistSink, TaskEventCommit};
use crate::runtime::events::identity::DispatchIdentity;
use crate::runtime::utils::now_ms;
use piko_protocol::PersistEvent;

pub(crate) async fn commit_persist_event(
    sink: &Arc<dyn PersistSink>,
    identity: &DispatchIdentity,
    _turn_id: &str,
    head_message_id: &Arc<Mutex<Option<String>>>,
    task_seq: &Arc<AtomicU64>,
    event: &PersistEvent,
) -> Result<u64, PersistError> {
    match event {
        PersistEvent::UserCommitted {
            session_id,
            message_id,
            task_id,
            agent_id,
            work_id,
            message,
        } => {
            let seq = task_seq.fetch_add(1, Ordering::Relaxed) + 1;
            let parent_message_id = head_message_id.lock().expect("head lock poisoned").clone();
            let commit = MessageCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                work_id: work_id.clone(),
                task_seq: seq,
                message_id: message_id.clone(),
                parent_message_id,
                message: message.clone(),
                committed_at: message_timestamp(message),
            };
            sink.commit_message(commit).await?;
            *head_message_id.lock().expect("head lock poisoned") = Some(message_id.clone());
            Ok(seq)
        }
        PersistEvent::Finalized {
            session_id,
            message_id,
            task_id,
            agent_id,
            work_id,
            message,
        } => {
            let seq = task_seq.fetch_add(1, Ordering::Relaxed) + 1;
            let parent_message_id = head_message_id.lock().expect("head lock poisoned").clone();
            let commit = MessageCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                work_id: work_id.clone(),
                task_seq: seq,
                message_id: message_id.clone(),
                parent_message_id,
                message: message.clone(),
                committed_at: message_timestamp(message),
            };
            sink.commit_message(commit).await?;
            *head_message_id.lock().expect("head lock poisoned") = Some(message_id.clone());
            Ok(seq)
        }
        PersistEvent::ToolCallCommitted {
            session_id,
            message_id,
            task_id,
            agent_id,
            work_id,
            parent_message_id,
            message,
        } => {
            let seq = task_seq.fetch_add(1, Ordering::Relaxed) + 1;
            let commit = MessageCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                work_id: work_id.clone(),
                task_seq: seq,
                message_id: message_id.clone(),
                parent_message_id: Some(parent_message_id.clone()),
                message: message.clone(),
                committed_at: message_timestamp(message),
            };
            sink.commit_message(commit).await?;
            *head_message_id.lock().expect("head lock poisoned") = Some(message_id.clone());
            Ok(seq)
        }
        PersistEvent::ToolResultCommitted {
            session_id,
            message_id,
            task_id,
            agent_id,
            work_id,
            message,
        } => {
            let seq = task_seq.fetch_add(1, Ordering::Relaxed) + 1;
            let parent_message_id = head_message_id.lock().expect("head lock poisoned").clone();
            let commit = MessageCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                work_id: work_id.clone(),
                task_seq: seq,
                message_id: message_id.clone(),
                parent_message_id,
                message: message.clone(),
                committed_at: message_timestamp(message),
            };
            sink.commit_message(commit).await?;
            *head_message_id.lock().expect("head lock poisoned") = Some(message_id.clone());
            Ok(seq)
        }
        PersistEvent::TaskEventCommitted(task_event) => {
            let seq = task_seq.fetch_add(1, Ordering::Relaxed) + 1;
            let commit = TaskEventCommit {
                session_id: identity.session_id().to_string(),
                task_id: identity.task_id().to_string(),
                agent_id: identity.agent_id().to_string(),
                task_seq: seq,
                event: task_event.clone(),
                committed_at: now_ms(),
            };
            sink.commit_task_event(commit).await?;
            Ok(seq)
        }
    }
}

fn message_timestamp(message: &Message) -> i64 {
    match message {
        Message::User { timestamp, .. }
        | Message::Assistant { timestamp, .. }
        | Message::ToolCall { timestamp, .. }
        | Message::ToolResult { timestamp, .. } => timestamp.unwrap_or_else(now_ms),
    }
}
