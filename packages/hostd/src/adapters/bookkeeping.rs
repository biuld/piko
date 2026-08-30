//! Adapter from the host session tree to orchd's F-04 estimator.

use piko_orchd::transcript::{message_tokens, text_tokens};
use piko_protocol::messages::ContentBlock;
use piko_protocol::session::{CustomMessageContent, SessionTreeEntry};

use crate::domain::bookkeeping::{ContextOccupancy, ContextUsageEstimate, occupancy};
use crate::ports::TranscriptEstimator;

#[derive(Debug, Default, Clone, Copy)]
pub struct OrchTranscriptEstimator;

impl TranscriptEstimator for OrchTranscriptEstimator {
    fn estimate_entry_tokens(&self, entry: &SessionTreeEntry) -> u64 {
        estimate_entry_tokens(entry)
    }
}

/// Raw F-04 text estimate (`ceil(bytes / 3)`). No per-message framing.
pub fn estimate_tokens(text: &str) -> u64 {
    text_tokens(text)
}

/// F-04 occupancy of one session-tree entry.
pub fn estimate_entry_tokens(entry: &SessionTreeEntry) -> u64 {
    match entry {
        SessionTreeEntry::Message(message) => message_tokens(&message.message),
        SessionTreeEntry::Compaction(compaction) => framed_text_tokens(&compaction.summary),
        SessionTreeEntry::BranchSummary(summary) => framed_text_tokens(&summary.summary),
        SessionTreeEntry::CustomMessage(custom) => {
            framed_text_tokens(&custom_message_text(&custom.content))
        }
        _ => 0,
    }
}

pub fn estimate_context_tokens(entries: &[SessionTreeEntry]) -> ContextUsageEstimate {
    let tokens = entries.iter().map(estimate_entry_tokens).sum();
    ContextUsageEstimate::from_tokens(tokens)
}

pub fn context_occupancy(
    entries: &[SessionTreeEntry],
    window: Option<u64>,
    last_usage: Option<&piko_protocol::messages::Usage>,
) -> ContextOccupancy {
    occupancy(estimate_context_tokens(entries).tokens, window, last_usage)
}

fn framed_text_tokens(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }
    text_tokens(text).saturating_add(16)
}

fn custom_message_text(content: &CustomMessageContent) -> String {
    match content {
        CustomMessageContent::String(text) => text.clone(),
        CustomMessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(content_block_text)
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn content_block_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text { text } => Some(text.clone()),
        ContentBlock::Thinking { thinking, .. } => Some(thinking.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::messages::{Message, MessageContent};
    use piko_protocol::session::MessageEntry;

    fn user_entry(text: &str) -> SessionTreeEntry {
        SessionTreeEntry::Message(MessageEntry {
            id: "u1".into(),
            parent_id: None,
            timestamp: "1".into(),
            agent_id: "main".into(),
            agent_instance_id: "agent-main".into(),
            root_input_id: "turn-1".into(),
            transcript_seq: 1,
            message: Message::User {
                content: MessageContent::String(text.into()),
                timestamp: None,
            },
        })
    }

    #[test]
    fn message_occupancy_matches_orchd_estimator() {
        let entry = user_entry("abcd");
        let SessionTreeEntry::Message(message) = &entry else {
            panic!("expected message");
        };
        assert_eq!(
            estimate_entry_tokens(&entry),
            piko_orchd::transcript::message_tokens(&message.message)
        );
        assert_eq!(estimate_entry_tokens(&entry), 18);
    }

    #[test]
    fn estimate_tokens_is_raw_f04_text_cost() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abc"), 1);
        assert_eq!(estimate_tokens("abcd"), 2);
        assert_eq!(estimate_tokens("你"), 1);
    }

    #[test]
    fn occupancy_keeps_provider_fill_separate_from_estimate() {
        let entries = [user_entry("abcd")];
        let mut usage = piko_protocol::messages::Usage::empty();
        usage.input = 40;
        usage.cache_read = 10;
        let snapshot = context_occupancy(&entries, Some(128_000), Some(&usage));
        assert_eq!(snapshot.estimated_tokens, 18);
        assert_eq!(snapshot.last_provider_fill, 50);
        assert_eq!(snapshot.remaining(), Some(127_982));
    }
}
