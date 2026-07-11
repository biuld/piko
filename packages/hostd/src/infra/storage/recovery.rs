//! Task-oriented session recovery helpers.
//!
//! Transcript facts come exclusively from `tasks/{task-id}.jsonl` shards.
//! Display/agent-view replay is projected separately from committed messages.

use piko_protocol::{Message, SessionTreeEntry, TaskSource};

use crate::api::{AgentTaskState, MessageEntry};

use super::task_repository::RecoveredTask;

/// Build session-tree message entries from a recovered task shard.
pub fn transcript_entries_from_recovered(recovered: &RecoveredTask) -> Vec<SessionTreeEntry> {
    recovered
        .transcript
        .iter()
        .map(|message| {
            SessionTreeEntry::Message(MessageEntry {
                id: message.id.clone(),
                parent_id: message.parent_id.clone(),
                timestamp: message.timestamp.to_string(),
                agent_id: message.agent_id.clone(),
                task_id: message.task_id.clone(),
                work_id: message.work_id.clone(),
                task_seq: message.task_seq,
                message: message.message.clone(),
            })
        })
        .collect()
}

/// Build ordered protocol messages for orchd task reattach.
pub fn transcript_messages_from_recovered(recovered: &RecoveredTask) -> Vec<Message> {
    let mut messages: Vec<(u64, Message)> = recovered
        .transcript
        .iter()
        .map(|entry| (entry.task_seq, entry.message.clone()))
        .collect();
    messages.sort_by_key(|(seq, _)| *seq);
    messages.into_iter().map(|(_, message)| message).collect()
}

/// Build ordered protocol messages for a single task from session-tree entries.
pub fn transcript_messages_from_entries(
    entries: &[SessionTreeEntry],
    task_id: &str,
) -> Vec<Message> {
    let mut messages: Vec<(u64, Message)> = entries
        .iter()
        .filter_map(|entry| match entry {
            SessionTreeEntry::Message(message) if message.task_id == task_id => {
                Some((message.task_seq, message.message.clone()))
            }
            _ => None,
        })
        .collect();
    messages.sort_by_key(|(seq, _)| *seq);
    messages.into_iter().map(|(_, message)| message).collect()
}

/// Build runtime task metadata for host state. `prompt` is audit-only and left empty.
pub fn agent_task_state_from_recovered(
    task_id: &str,
    recovered: &RecoveredTask,
    source: TaskSource,
) -> AgentTaskState {
    AgentTaskState {
        id: task_id.to_string(),
        target_agent_id: recovered.metadata.agent_id.clone(),
        prompt: String::new(),
        source,
        status: recovered.metadata.status.clone(),
        priority: 0,
        parent_task_id: recovered.metadata.parent_task_id.clone(),
        result: None,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use piko_protocol::agents::AgentTaskStatus;
    use piko_protocol::messages::MessageContent;

    use super::*;
    use crate::infra::storage::task_repository::TaskManifestEntry;

    fn sample_recovered() -> RecoveredTask {
        RecoveredTask {
            metadata: TaskManifestEntry {
                agent_id: "main".into(),
                parent_task_id: None,
                status: AgentTaskStatus::Idle,
                created_at: 1,
                updated_at: 2,
            },
            transcript: vec![super::super::task_repository::CommittedMessage {
                id: "msg-1".into(),
                parent_id: None,
                task_id: "task-1".into(),
                agent_id: "main".into(),
                work_id: "turn-1".into(),
                task_seq: 1,
                timestamp: 1,
                message: Message::User {
                    content: MessageContent::String("hello".into()),
                    timestamp: Some(1),
                },
            }],
            head_message_id: Some("msg-1".into()),
            last_task_seq: 1,
            lifecycle: Vec::new(),
            work_lifecycle: Vec::new(),
        }
    }

    #[test]
    fn recovered_task_state_does_not_copy_prompt_from_transcript() {
        let recovered = sample_recovered();
        let state = agent_task_state_from_recovered("task-1", &recovered, TaskSource::User);
        assert!(state.prompt.is_empty());
        assert_eq!(state.target_agent_id, "main");
    }

    #[test]
    fn transcript_messages_preserve_task_seq_order() {
        let recovered = RecoveredTask {
            transcript: vec![
                super::super::task_repository::CommittedMessage {
                    id: "msg-2".into(),
                    parent_id: Some("msg-1".into()),
                    task_id: "task-1".into(),
                    agent_id: "main".into(),
                    work_id: "turn-1".into(),
                    task_seq: 2,
                    timestamp: 2,
                    message: Message::Assistant {
                        content: vec![],
                        api: "test".into(),
                        provider: "test".into(),
                        model: "test".into(),
                        usage: None,
                        stop_reason: None,
                        error_message: None,
                        timestamp: Some(2),
                    },
                },
                super::super::task_repository::CommittedMessage {
                    id: "msg-1".into(),
                    parent_id: None,
                    task_id: "task-1".into(),
                    agent_id: "main".into(),
                    work_id: "turn-1".into(),
                    task_seq: 1,
                    timestamp: 1,
                    message: Message::User {
                        content: MessageContent::String("hello".into()),
                        timestamp: Some(1),
                    },
                },
            ],
            ..sample_recovered()
        };
        let messages = transcript_messages_from_recovered(&recovered);
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0], Message::User { .. }));
        assert!(matches!(messages[1], Message::Assistant { .. }));
    }
}
