//! Window shell: Primary Surface frames, island chrome, overlay host, widgets.
//!
//! - [`workbench`] — session Primary Surface (TitleBar, body, StatusBar)
//! - [`settings`] — settings Primary Surface frame only (TitleBar, body slots)
//! - [`island`] — IslandPanel shell (Workbench building block)
//! - [`overlay`] — OverlayHost surface (above all surfaces)
//! - [`widgets`] — shared presentational primitives (tree list rows)
//!
//! Product Settings / Palette / prompts / islands live under `crate::features`.
//! PrimarySurface state lives under `crate::app::primary_surface`.

pub mod island;
pub mod overlay;
pub mod settings;
pub mod widgets;
pub mod workbench;

pub use island::{
    FocusCycleDir, FocusReason, IslandContentViewport, IslandFocusRing, IslandHeader, IslandMsg,
    IslandPanel, IslandPlaceholder, IslandSessionPhase, focus_order,
};
#[allow(unused_imports)] // public override API for islands
pub use island::{IslandBody, IslandMedia};
pub use overlay::{
    EscapeOutcome, LocalConfirmKind, OverlayHost, OverlayLayer, OverlayPanelSpec,
    OverlayPanelStyle, TransientKind, render_overlay_layer,
};
pub use settings::mount_frame as mount_settings_frame;
pub use widgets::{
    TreeClickHandler, TreeContextMenuBuilder, TreeRowAccessory, TreeRowSpec, render_tree_list,
};
pub use workbench::{IslandId, mount_frame as mount_workbench_frame, render_right_column};
