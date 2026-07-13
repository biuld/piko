//! Agent-oriented session recovery helpers.
//!
//! Transcript facts come exclusively from `agents/{agent_instance_id}.jsonl`
//! shards. Display/agent-view replay is projected separately from committed
//! messages.

use piko_protocol::{Message, SessionTreeEntry};

use crate::api::MessageEntry;

use super::session_store::RecoveredAgent;

/// Build session-tree message entries from a recovered AgentInstance shard.
pub fn agent_transcript_entries(recovered: &RecoveredAgent) -> Vec<SessionTreeEntry> {
    recovered
        .transcript
        .iter()
        .map(|message| {
            SessionTreeEntry::Message(MessageEntry {
                id: message.id.clone(),
                parent_id: message.parent_id.clone(),
                timestamp: message.timestamp.to_string(),
                agent_id: message.agent_spec_id.clone(),
                agent_instance_id: message.agent_instance_id.clone(),
                source_turn_id: message.source_turn_id.clone().unwrap_or_default(),
                transcript_seq: message.transcript_seq,
                message: message.message.clone(),
            })
        })
        .collect()
}

/// Build ordered protocol messages for orchd agent reattach.
pub fn transcript_messages_from_agent(recovered: &RecoveredAgent) -> Vec<Message> {
    recovered
        .transcript
        .iter()
        .map(|entry| entry.message.clone())
        .collect()
}

/// Build ordered protocol messages for a single AgentInstance from session-tree entries.
pub fn transcript_messages_from_entries(
    entries: &[SessionTreeEntry],
    agent_instance_id: &str,
) -> Vec<Message> {
    let mut messages: Vec<(u64, Message)> = entries
        .iter()
        .filter_map(|entry| match entry {
            SessionTreeEntry::Message(message)
                if message.agent_instance_id == agent_instance_id =>
            {
                Some((message.transcript_seq, message.message.clone()))
            }
            _ => None,
        })
        .collect();
    messages.sort_by_key(|(seq, _)| *seq);
    messages.into_iter().map(|(_, message)| message).collect()
}

#[cfg(test)]
mod tests {
    use piko_protocol::messages::MessageContent;

    use super::*;
    use crate::infra::storage::session_store::CommittedMessage;

    fn sample_recovered() -> RecoveredAgent {
        RecoveredAgent {
            session_id: "session-1".into(),
            agent_instance_id: "agent-1".into(),
            agent_spec_id: "main".into(),
            transcript: vec![CommittedMessage {
                id: "msg-1".into(),
                parent_id: None,
                agent_instance_id: "agent-1".into(),
                agent_spec_id: "main".into(),
                execution_id: Some("exec-1".into()),
                source_turn_id: Some("turn-1".into()),
                transcript_seq: 1,
                timestamp: 1,
                message: Message::User {
                    content: MessageContent::String("hello".into()),
                    timestamp: Some(1),
                },
            }],
            head_message_id: Some("msg-1".into()),
            last_transcript_seq: 1,
        }
    }

    #[test]
    fn transcript_messages_preserve_transcript_seq_order() {
        let recovered = RecoveredAgent {
            transcript: vec![
                CommittedMessage {
                    id: "msg-2".into(),
                    parent_id: Some("msg-1".into()),
                    agent_instance_id: "agent-1".into(),
                    agent_spec_id: "main".into(),
                    execution_id: Some("exec-1".into()),
                    source_turn_id: Some("turn-1".into()),
                    transcript_seq: 2,
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
                CommittedMessage {
                    id: "msg-1".into(),
                    parent_id: None,
                    agent_instance_id: "agent-1".into(),
                    agent_spec_id: "main".into(),
                    execution_id: Some("exec-1".into()),
                    source_turn_id: Some("turn-1".into()),
                    transcript_seq: 1,
                    timestamp: 1,
                    message: Message::User {
                        content: MessageContent::String("hello".into()),
                        timestamp: Some(1),
                    },
                },
            ],
            ..sample_recovered()
        };
        let messages = transcript_messages_from_agent(&recovered);
        assert_eq!(messages.len(), 2);
        // Recovered transcript preserves on-disk append order (msg-2 first here).
        assert!(matches!(messages[0], Message::Assistant { .. }));
        assert!(matches!(messages[1], Message::User { .. }));
    }

    #[test]
    fn agent_transcript_entries_carry_identity_fields() {
        let recovered = sample_recovered();
        let entries = agent_transcript_entries(&recovered);
        assert_eq!(entries.len(), 1);
        let SessionTreeEntry::Message(entry) = &entries[0] else {
            panic!("expected message entry");
        };
        assert_eq!(entry.agent_instance_id, "agent-1");
        assert_eq!(entry.agent_id, "main");
        assert_eq!(entry.source_turn_id, "turn-1");
    }
}
