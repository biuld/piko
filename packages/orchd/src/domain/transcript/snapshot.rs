// ---- Domain: transcript copy-on-write snapshots ----
//
// An immutable view of the transcript plus its per-message token estimates
// (F-04 / D-04). Cloning a snapshot is two Arc bumps, so model steps,
// telemetry, and the budget preflight can share one allocation until the
// next mutation.

use std::sync::Arc;

use piko_protocol::messages::Message;

#[derive(Debug, Clone)]
pub struct TranscriptSnapshot {
    messages: Arc<Vec<Message>>,
    tokens: Arc<Vec<u64>>,
    total_tokens: u64,
}

impl TranscriptSnapshot {
    pub fn new(messages: Vec<Message>, tokens: Vec<u64>) -> Self {
        let total_tokens = tokens.iter().sum();
        Self {
            messages: Arc::new(messages),
            tokens: Arc::new(tokens),
            total_tokens,
        }
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn tokens(&self) -> &[u64] {
        &self.tokens
    }

    pub fn total_tokens(&self) -> u64 {
        self.total_tokens
    }

    pub fn into_messages(self) -> Vec<Message> {
        Arc::try_unwrap(self.messages).unwrap_or_else(|shared| (*shared).clone())
    }

    /// True when two snapshots share the underlying message allocation.
    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.messages, &other.messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_shares_storage() {
        let snapshot = TranscriptSnapshot::new(vec![], vec![]);
        let clone = snapshot.clone();
        assert!(snapshot.shares_storage_with(&clone));
    }

    #[test]
    fn total_is_sum_of_estimates() {
        let snapshot = TranscriptSnapshot::new(vec![], vec![10, 20, 30]);
        assert_eq!(snapshot.total_tokens(), 60);
        assert_eq!(snapshot.tokens(), &[10, 20, 30]);
    }

    #[test]
    fn into_messages_round_trips() {
        let message = Message::User {
            content: piko_protocol::messages::MessageContent::String("hi".into()),
            timestamp: None,
        };
        let snapshot = TranscriptSnapshot::new(vec![message.clone()], vec![1]);
        assert_eq!(snapshot.into_messages(), vec![message]);
    }
}
