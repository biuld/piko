#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalConfirmKind {
    QuitBusy,
    DeleteSession {
        session_id: String,
        display_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransientKind {
    CommandPalette,
    SessionRename {
        session_id: String,
        initial_name: String,
    },
}

/// Which chrome overlay layer is currently interactive (highest priority wins).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayLayer {
    HostPrompt,
    LocalConfirm(LocalConfirmKind),
    Transient(TransientKind),
}
