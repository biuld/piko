//! Shared chrome: Primary Surfaces, island shell, overlay stack, widgets.
//!
//! - [`primary_surface`] — Workbench vs Settings frame state
//! - [`workbench`] — session Primary Surface (TitleBar, body, StatusBar)
//! - [`settings`] — settings Primary Surface (TitleBar, nav body)
//! - [`island`] — IslandPanel shell (Workbench building block)
//! - [`overlay`] — OverlayHost surface + Command Palette (above all surfaces)
//! - [`widgets`] — shared presentational primitives (tree list rows)

pub mod island;
pub mod overlay;
pub mod primary_surface;
pub mod settings;
pub mod widgets;
pub mod workbench;

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
pub use primary_surface::{PrimarySurface, SettingsSection};
pub use settings::mount_frame as mount_settings_frame;
pub use widgets::{TreeClickHandler, TreeRowAccessory, TreeRowSpec, render_tree_list};
pub use workbench::{IslandId, mount_frame as mount_workbench_frame, render_right_column};
