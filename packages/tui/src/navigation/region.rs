//! Product paint region ids (`R` for `piko_tui_layout::Node<R>`).

use super::SurfaceId;

/// Paint target inside the shell **body** (chrome is not a region).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Region {
    // ── Plane (workspace) ────────────────────────────────────────────────
    /// Conversation / stream viewport (primary reading surface).
    Stream,
    /// Reserved blank row between Stream and the Dock Stack.
    DockBoundary,
    /// Completion list above the composer.
    Suggest,
    /// Resident notice / contextual-hint projection above the composer.
    Guidance,
    /// Text / composer.
    Composer,

    // ── Modal trees ──────────────────────────────────────────────────────
    /// Content of an open surface hosted in a [`piko_tui_layout::ModalLayer`].
    Surface(SurfaceId),
}
