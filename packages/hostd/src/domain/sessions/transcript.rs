//! Pure session-tree transcript projection helpers.

use crate::api::{Message, MessageContent, SessionTreeEntry};
use crate::domain::compaction::context_entries_after_compaction;

/// Build ordered protocol messages from all session-tree message entries.
///
/// Used when each Turn uses a distinct Execution shard: model context must
/// span the whole conversation, not a single shard.
pub fn transcript_messages_from_session_entries(entries: &[SessionTreeEntry]) -> Vec<Message> {
    context_entries_after_compaction(entries)
        .iter()
        .filter_map(|entry| match entry {
            SessionTreeEntry::Message(message) => Some(message.message.clone()),
            SessionTreeEntry::Compaction(compaction) => Some(Message::Context {
                content: MessageContent::String(format!(
                    "Compaction summary of earlier conversation:\n{}",
                    compaction.summary
                )),
                trust: piko_protocol::ContentTrust::Untrusted,
                source: piko_protocol::PromptSource::new("compaction", &compaction.id),
                timestamp: None,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_compaction_replaces_older_messages_in_model_context() {
        let message = |id: &str, parent_id: Option<&str>, text: &str| {
            SessionTreeEntry::Message(piko_protocol::MessageEntry {
                id: id.into(),
                parent_id: parent_id.map(str::to_string),
                timestamp: String::new(),
                agent_id: "main".into(),
                agent_instance_id: "root".into(),
                source_turn_id: String::new(),
                transcript_seq: 0,
                message: Message::User {
                    content: MessageContent::String(text.into()),
                    timestamp: None,
                },
            })
        };
        let entries = vec![
            message("old", None, "discard me"),
            message("kept", Some("old"), "keep me"),
            SessionTreeEntry::Compaction(piko_protocol::CompactionEntry {
                id: "compact".into(),
                parent_id: Some("kept".into()),
                timestamp: String::new(),
                summary: "summary".into(),
                first_kept_entry_id: "kept".into(),
                tokens_before: 10,
                details: None,
                from_hook: None,
            }),
        ];

        let transcript = transcript_messages_from_session_entries(&entries);
        assert_eq!(transcript.len(), 2);
        assert!(matches!(
            &transcript[0],
            Message::Context { content: MessageContent::String(text), .. }
                if text.contains("summary") && !text.contains("discard me")
        ));
    }
}
