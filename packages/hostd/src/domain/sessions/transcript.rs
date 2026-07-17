//! Pure session-tree transcript projection helpers.

use crate::api::{Message, SessionTreeEntry};

/// Build ordered protocol messages from all session-tree message entries.
///
/// Used when each Turn uses a distinct Execution shard: model context must
/// span the whole conversation, not a single shard.
pub fn transcript_messages_from_session_entries(entries: &[SessionTreeEntry]) -> Vec<Message> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            SessionTreeEntry::Message(message) => Some(message.message.clone()),
            _ => None,
        })
        .collect()
}
