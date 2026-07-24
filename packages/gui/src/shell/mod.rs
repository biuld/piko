//! Window shell: Archipelago frames, product overlay host, Workbench assembly.
//!
//! Shared Islands chrome (panel, theme surfaces, generic layout tree, overlay
//! panel geometry, tree widgets) lives in **`piko-chrome`**. This module owns
//! piko product wiring on top of that kit:
//!
//! - [`workbench`] — Workbench archipelago (TitleBar, body, StatusBar)
//! - [`settings`] — Settings archipelago frame only (TitleBar, body slots)
//! - [`island`] — product `IslandMsg` / phase + re-exports of chrome panel/focus
//! - [`overlay`] — product OverlayHost stack (kinds, prompts) + chrome surface
//! - [`widgets`] — re-export of chrome tree-list widgets
//!
//! Product Settings / Palette / prompts / islands live under `crate::features`.
//! Archipelago router lives under `crate::app::archipelago`.

pub mod island;
pub mod overlay;
pub mod settings;
pub mod widgets;
pub mod workbench;

pub use island::{
    FocusCycleDir, FocusReason, IslandContentViewport, IslandFocusRing, IslandFocusTable,
    IslandHeader, IslandHost, IslandMessage, IslandMsg, IslandPanel, IslandPlaceholder,
    IslandSessionPhase, IslandView, activate_focus_handle, route_focus_message,
    schedule_island_message,
};
#[allow(unused_imports)] // public override API for islands
pub use island::{IslandBody, IslandMedia};
pub use overlay::{
    EscapeOutcome, LocalConfirmKind, OverlayHost, OverlayLayer, OverlayPanelSpec,
    OverlayPanelStyle, TransientKind, render_overlay_layer,
};
pub use settings::SettingsFrameChrome;
pub use settings::mount_frame as mount_settings_frame;
pub use widgets::{
    TreeClickHandler, TreeContextMenuBuilder, TreeRowAccessory, TreeRowSpec, render_tree_list,
};
pub use workbench::{
    ALL_ISLAND_IDS, IslandId, mount_frame as mount_workbench_frame, render_right_column,
};
