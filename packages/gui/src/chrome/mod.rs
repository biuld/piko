//! Shared Workbench chrome: window bars, column assembly, island shell, overlay, widgets.
//!
//! - [`workbench`] — left / center / right column assembly + layout state
//! - [`island`] — IslandPanel shell (header, viewport, focus, messaging)
//! - [`overlay`] — OverlayHost surface + Command Palette
//! - [`widgets`] — shared presentational primitives (tree list rows)

pub mod island;
pub mod overlay;
pub mod widgets;
pub mod workbench;

mod status_bar;
mod title_bar;

pub use island::{
    FocusCycleDir, IslandFocusRing, IslandHeader, IslandMsg, IslandPanel, IslandPlaceholder,
    IslandSessionPhase, focus_order,
};
#[allow(unused_imports)] // public override API for islands
pub use island::{IslandBody, IslandMedia};
pub use overlay::{
    CommandPalette, EscapeOutcome, LocalConfirmKind, OverlayHost, OverlayLayer, OverlayPanelSpec,
    OverlayPanelStyle, PaletteConfirm, PaletteSelectNext, PaletteSelectPrev, TransientKind,
    render_overlay_layer,
};
pub use status_bar::render_status_bar;
pub use title_bar::render_title_bar;
pub use widgets::{TreeClickHandler, TreeRowAccessory, TreeRowSpec, render_tree_list};
pub use workbench::{IslandId, render_right_column};
