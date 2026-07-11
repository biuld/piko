// ---- Domain: transcript — in-memory message history ----

pub use piko_protocol::messages::{ContentBlock, Message, MessageContent, Usage as MessageUsage};

/// Manages the local transcript of an agent task, tracking user inputs,
/// assistant outputs, and tool calls.
pub struct TranscriptManager {
    messages: Vec<Message>,
}

impl TranscriptManager {
    pub fn new(history: Option<Vec<Message>>) -> Self {
        Self {
            messages: history.unwrap_or_default(),
        }
    }

    pub fn push_user_content(&mut self, content: MessageContent, timestamp: Option<i64>) {
        self.messages.push(Message::User { content, timestamp });
    }

    pub fn push_assistant(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn push_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn to_vec(&self) -> Vec<Message> {
        self.messages.clone()
    }

    pub fn checkpoint(&self) -> usize {
        self.messages.len()
    }

    pub fn rollback(&mut self, checkpoint: usize) {
        self.messages.truncate(checkpoint);
    }
}
