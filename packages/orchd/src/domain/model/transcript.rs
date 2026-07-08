// ---- Domain: model transcript — message types (re-exports from piko_protocol) ----

pub use piko_protocol::messages::{
    ContentBlock, Message, MessageContent, Usage as MessageUsage, UsageCost as MessageUsageCost,
};

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

    pub fn push_user(&mut self, text: String) {
        if !text.trim().is_empty() {
            self.messages.push(Message::User {
                content: MessageContent::String(text),
                timestamp: None,
            });
        }
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
}
