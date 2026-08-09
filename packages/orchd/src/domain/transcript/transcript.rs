// ---- Domain: transcript — in-memory message history ----

use std::sync::Arc;

pub use piko_protocol::messages::{ContentBlock, Message, MessageContent, Usage as MessageUsage};

use super::normalize::{NormalizedTranscript, TranscriptPolicy, normalize};
use super::snapshot::TranscriptSnapshot;
use super::tokens::{estimate_messages, message_tokens};

/// Manages the local transcript of an agent task, tracking user inputs,
/// assistant outputs, and tool calls (F-04 / D-04). Messages are committed
/// in call/item order and carry a per-message token estimate; snapshots give
/// cheap copy-on-write views that are invalidated by any mutation.
pub struct TranscriptManager {
    messages: Vec<Message>,
    tokens: Vec<u64>,
    generation: u64,
    raw_snapshot: Option<Arc<TranscriptSnapshot>>,
}

impl TranscriptManager {
    pub fn new(history: Option<Vec<Message>>) -> Self {
        let history = history.unwrap_or_default();
        let tokens = estimate_messages(&history);
        Self {
            messages: history,
            tokens,
            generation: 0,
            raw_snapshot: None,
        }
    }

    pub fn push_user_content(&mut self, content: MessageContent, timestamp: Option<i64>) {
        self.push_message(Message::User { content, timestamp });
    }

    pub fn push_assistant(&mut self, message: Message) {
        self.push_message(message);
    }

    pub fn push_message(&mut self, message: Message) {
        self.tokens.push(message_tokens(&message));
        self.messages.push(message);
        self.invalidate_snapshot();
    }

    pub fn to_vec(&self) -> Vec<Message> {
        self.messages.clone()
    }

    /// Per-message token estimates, aligned with `to_vec()`.
    pub fn tokens(&self) -> &[u64] {
        &self.tokens
    }

    /// Sum of the tracked per-message estimates.
    pub fn total_tokens(&self) -> u64 {
        self.tokens.iter().sum()
    }

    /// Copy-on-write snapshot of the committed transcript. Repeat calls
    /// before the next mutation return the same allocation.
    pub fn snapshot(&mut self) -> Arc<TranscriptSnapshot> {
        if let Some(cached) = &self.raw_snapshot {
            return cached.clone();
        }
        let snapshot = Arc::new(TranscriptSnapshot::new(
            self.messages.clone(),
            self.tokens.clone(),
        ));
        self.raw_snapshot = Some(snapshot.clone());
        snapshot
    }

    /// Normalized model view: committed messages projected through the
    /// policy (tool-output truncation) with fresh estimates. Never mutates
    /// the manager.
    pub fn model_view(&self, policy: &TranscriptPolicy) -> NormalizedTranscript {
        let (messages, truncated_outputs) = normalize(&self.messages, policy);
        NormalizedTranscript::new(messages, truncated_outputs)
    }

    pub fn checkpoint(&self) -> usize {
        self.messages.len()
    }

    pub fn rollback(&mut self, checkpoint: usize) {
        self.messages.truncate(checkpoint);
        self.tokens.truncate(checkpoint);
        self.invalidate_snapshot();
    }

    /// Drop everything before the most recent user message (F-05
    /// `new_context_window` fresh-window semantics for the running
    /// execution). With no user message the transcript clears.
    pub fn reset_to_recent_user(&mut self) {
        let start = self
            .messages
            .iter()
            .rposition(|message| matches!(message, Message::User { .. }))
            .unwrap_or(self.messages.len());
        self.messages.drain(..start);
        self.tokens.drain(..start);
        self.invalidate_snapshot();
    }

    fn invalidate_snapshot(&mut self) {
        self.generation += 1;
        self.raw_snapshot = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_message(text: &str) -> Message {
        Message::User {
            content: MessageContent::String(text.into()),
            timestamp: None,
        }
    }

    fn assistant_message(text: &str) -> Message {
        Message::Assistant {
            content: vec![ContentBlock::Text { text: text.into() }],
            checkpoint: None,
            provider: "test".into(),
            model: "test".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        }
    }

    #[test]
    fn total_tokens_tracks_pushes_and_rollback() {
        let mut manager = TranscriptManager::new(None);
        assert_eq!(manager.total_tokens(), 0);
        manager.push_user_content(MessageContent::String("hello world".into()), None);
        let after_first = manager.total_tokens();
        assert!(after_first > 0);
        let checkpoint = manager.checkpoint();
        manager.push_assistant(text_message("second"));
        let after_second = manager.total_tokens();
        assert_eq!(manager.tokens().len(), 2);
        assert_eq!(manager.tokens().iter().sum::<u64>(), after_second);

        manager.push_assistant(text_message("third"));
        assert_eq!(manager.tokens().len(), 3);
        manager.rollback(checkpoint);
        assert_eq!(manager.tokens().len(), 1);
        assert_eq!(manager.total_tokens(), after_first);
    }

    #[test]
    fn snapshot_is_shared_until_mutation() {
        let mut manager = TranscriptManager::new(Some(vec![text_message("a"), text_message("b")]));
        let first = manager.snapshot();
        let second = manager.snapshot();
        assert!(first.shares_storage_with(&second));
        assert_eq!(first.total_tokens(), manager.total_tokens());

        manager.push_message(text_message("c"));
        let third = manager.snapshot();
        assert!(!first.shares_storage_with(&third));
        assert_eq!(third.total_tokens(), manager.total_tokens());
        assert_eq!(third.messages().len(), 3);
    }

    #[test]
    fn rollback_invalidates_snapshot() {
        let mut manager = TranscriptManager::new(None);
        manager.push_message(text_message("a"));
        let checkpoint = manager.checkpoint();
        manager.push_message(text_message("b"));
        let before = manager.snapshot();
        manager.rollback(checkpoint);
        let after = manager.snapshot();
        assert!(!before.shares_storage_with(&after));
        assert_eq!(after.messages().len(), 1);
    }

    #[test]
    fn reset_to_recent_user_keeps_latest_user_and_later_messages() {
        let mut manager = TranscriptManager::new(None);
        manager.push_user_content(MessageContent::String("first".into()), None);
        manager.push_assistant(assistant_message("old assistant"));
        manager.push_user_content(MessageContent::String("second".into()), None);
        manager.push_assistant(assistant_message("new assistant"));

        manager.reset_to_recent_user();

        let messages = manager.to_vec();
        assert_eq!(messages.len(), 2);
        assert!(matches!(
            &messages[0],
            Message::User {
                content: MessageContent::String(text),
                ..
            } if text == "second"
        ));
        assert_eq!(manager.tokens().len(), 2);
        assert_eq!(manager.total_tokens(), manager.tokens().iter().sum::<u64>());
    }

    #[test]
    fn reset_to_recent_user_clears_without_user_message() {
        let mut manager = TranscriptManager::new(None);
        manager.push_assistant(assistant_message("assistant only"));
        manager.reset_to_recent_user();
        assert!(manager.to_vec().is_empty());
        assert_eq!(manager.total_tokens(), 0);
    }

    #[test]
    fn model_view_normalizes_and_reports_truncation() {
        let mut manager = TranscriptManager::new(None);
        manager.push_message(Message::ToolResult {
            tool_call_id: "call-1".into(),
            tool_name: Some("bash".into()),
            content: vec![ContentBlock::Text {
                text: "x".repeat(100_000),
            }],
            details: Some(serde_json::json!({ "full": "x".repeat(100_000) })),
            is_error: Some(false),
            timestamp: None,
        });
        let view = manager.model_view(&TranscriptPolicy::default());
        assert_eq!(view.truncated_outputs, 1);
        // Committed transcript is untouched.
        assert_eq!(manager.to_vec().len(), 1);
        assert!(matches!(&manager.to_vec()[0], Message::ToolResult { .. }));

        // Snapshot accounting matches re-estimating the projected messages.
        let projected = view.snapshot.messages();
        let expected: u64 = estimate_messages(projected).iter().sum();
        assert_eq!(view.snapshot.total_tokens(), expected);
    }
}
