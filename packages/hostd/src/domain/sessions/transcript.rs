//! Pure session-tree transcript projection helpers.

use crate::api::{CustomMessageContent, Message, MessageContent, SessionTreeEntry};
use crate::domain::compaction::{active_branch_entries, context_entries_after_compaction};

/// Build ordered protocol messages from all session-tree message entries.
///
/// Model context spans the selected session branch rather than physical
/// commit order.
pub fn transcript_messages_from_session_entries(
    entries: &[SessionTreeEntry],
    selected_entry_id: Option<&str>,
) -> Vec<Message> {
    let branch = active_branch_entries(entries, selected_entry_id);
    context_entries_after_compaction(&branch)
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
            SessionTreeEntry::BranchSummary(summary) => Some(Message::Context {
                content: MessageContent::String(format!(
                    "Summary of the abandoned conversation branch:\n{}",
                    summary.summary
                )),
                trust: piko_protocol::ContentTrust::Untrusted,
                source: piko_protocol::PromptSource::new("branch_summary", &summary.id),
                timestamp: None,
            }),
            SessionTreeEntry::CustomMessage(custom) => Some(Message::Context {
                content: match &custom.content {
                    CustomMessageContent::String(text) => MessageContent::String(text.clone()),
                    CustomMessageContent::Blocks(blocks) => MessageContent::Blocks(blocks.clone()),
                },
                trust: piko_protocol::ContentTrust::Untrusted,
                source: piko_protocol::PromptSource::new("custom_message", &custom.id),
                timestamp: None,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, parent_id: Option<&str>, text: &str) -> SessionTreeEntry {
        SessionTreeEntry::Message(piko_protocol::MessageEntry {
            id: id.into(),
            parent_id: parent_id.map(str::to_string),
            timestamp: String::new(),
            agent_id: "main".into(),
            agent_instance_id: "root".into(),
            root_input_id: String::new(),
            transcript_seq: 0,
            message: Message::User {
                content: MessageContent::String(text.into()),
                timestamp: None,
            },
        })
    }

    #[test]
    fn latest_compaction_replaces_older_messages_in_model_context() {
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

        let transcript = transcript_messages_from_session_entries(&entries, Some("compact"));
        assert_eq!(transcript.len(), 2);
        assert!(matches!(
            &transcript[0],
            Message::Context { content: MessageContent::String(text), .. }
                if text.contains("summary") && !text.contains("discard me")
        ));
    }

    #[test]
    fn empty_cursor_has_no_model_context() {
        let entries = vec![message("old", None, "abandoned")];
        assert!(transcript_messages_from_session_entries(&entries, None).is_empty());
    }

    #[test]
    fn branch_summary_is_model_visible_context() {
        let entries = vec![SessionTreeEntry::BranchSummary(
            piko_protocol::BranchSummaryEntry {
                id: "summary".into(),
                parent_id: None,
                timestamp: "1".into(),
                from_id: "old-tip".into(),
                summary: "preserved decision".into(),
                details: None,
                from_hook: None,
            },
        )];
        let transcript = transcript_messages_from_session_entries(&entries, Some("summary"));
        assert!(matches!(
            &transcript[..],
            [Message::Context { content: MessageContent::String(text), source, .. }]
                if text.contains("preserved decision") && source.kind == "branch_summary"
        ));
    }
}
