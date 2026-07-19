//! Overlay kind tags and visible layer selection.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalConfirmKind {
    QuitBusy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransientKind {
    CommandPalette,
}

/// Which chrome overlay layer is currently interactive (highest priority wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayLayer {
    HostPrompt,
    LocalConfirm(LocalConfirmKind),
    Transient(TransientKind),
}
