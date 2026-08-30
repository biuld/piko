use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(crate) struct PendingSubmissionUi {
    pub draft: crate::features::editor::state::EditorDraft,
}

/// Correlates in-flight host commands by `command_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingCommandKind {
    BootstrapConfig,
    BootstrapCatalog,
    /// Silent `ModelList` for catalog cache (no Models panel).
    BootstrapModels,
    /// Interactive model picker open.
    ModelList,
    SessionCreate,
    SessionList,
    SessionOpen,
    AgentInputSubmit,
    SessionDelete,
    UsageRefresh,
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

    pub fn contains_kind(&self, kind: PendingCommandKind) -> bool {
        self.by_id.values().any(|pending| *pending == kind)
    }
}
