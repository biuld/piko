use std::collections::HashMap;

/// Correlates in-flight host commands by `command_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingCommandKind {
    SessionList,
    SessionOpen,
    TurnSubmit,
    SessionDelete,
}

#[derive(Debug, Clone, Default)]
pub struct PendingCommands {
    by_id: HashMap<String, PendingCommandKind>,
    /// Session id targeted by an in-flight delete (UI clears only after success).
    pub delete_session_id: Option<String>,
}

impl PendingCommands {
    pub fn track(&mut self, command_id: String, kind: PendingCommandKind) {
        self.by_id.insert(command_id, kind);
    }

    pub fn take(&mut self, command_id: &str) -> Option<PendingCommandKind> {
        self.by_id.remove(command_id)
    }

    pub fn clear_kind(&mut self, kind: PendingCommandKind) {
        self.by_id.retain(|_, k| *k != kind);
    }
}
