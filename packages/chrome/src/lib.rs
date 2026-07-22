//! GPUI Islands chrome kit.
//!
//! ```text
//! src/
//!   runtime/   archipelago · island contracts · layout trees
//!   chrome/    panel shell · overlay · list/tree widgets
//!   theme/     tokens · metrics · typography · icons
//!   assets/    embedded SVG AssetSource
//! ```
//!
//! **Does not own** product archipelago/island ids, domain messages, backend
//! bridges, or application frame assembly.
//!
//! Crate-root modules (`island`, `layout`, `overlay`, …) re-export the stable
//! consumer API; internal layout may change without moving those paths.

pub mod assets;
pub mod chrome;
pub mod runtime;
pub mod theme;

// ── Stable module paths for consumers ──────────────────────────────────────

/// Island contracts + panel shell (runtime + paint).
pub mod island {
    pub use crate::chrome::panel::{
        IslandBody, IslandContentViewport, IslandHeader, IslandMedia, IslandPanel,
        IslandPlaceholder,
    };
    pub use crate::runtime::island::{
        FocusCycleDir, FocusMsg, FocusReason, FocusRing, FocusTransition, IslandFocusSlot,
        IslandFocusTable, IslandHost, IslandMessage, IslandView, UnknownIsland,
        activate_focus_handle, route_focus_message, schedule_island_message,
    };
}

/// Generic split trees.
pub mod layout {
    pub use crate::runtime::layout::*;
}

/// Archipelago router and workspace.
pub mod archipelago {
    pub use crate::runtime::archipelago::*;
}

/// Overlay panel geometry and focus session.
pub mod overlay {
    pub use crate::chrome::overlay::*;
}

/// List/tree widgets and list keyboard.
pub mod widgets {
    pub use crate::chrome::list::*;
}

// ── Flat re-exports (optional `use piko_chrome::*`) ─────────────────────────

pub use assets::ChromeAssets;

pub use archipelago::{
    ArchipelagoMessage, ArchipelagoNav, ArchipelagoRouter, ArchipelagoTransition,
    ArchipelagoWorkspace, ChromeRoute, ChromeRouteOutcome, activate_archipelago_islands,
    route_archipelago_nav, route_chrome_message,
};
pub use island::{
    FocusCycleDir, FocusMsg, FocusReason, FocusRing, FocusTransition, IslandBody,
    IslandContentViewport, IslandFocusSlot, IslandFocusTable, IslandHeader, IslandHost,
    IslandMedia, IslandMessage, IslandPanel, IslandPlaceholder, IslandView, UnknownIsland,
    activate_focus_handle, route_focus_message, schedule_island_message,
};
pub use layout::{IslandAxis, IslandNode, prune_island_tree};
pub use overlay::{
    OverlayEnvelope, OverlayFocusSession, OverlayPanelSpec, OverlayPanelStyle, overlay_envelope,
    render_overlay_layer,
};
pub use theme::{
    ChromeIcon, ChromePalette, ChromeTokens, IconSize, PanelSide, RoleAccent, TextRole,
    ThemeSnapshot, apply_chrome_dark_theme, apply_chrome_theme, body_markdown, chrome_palette,
    disclosure, icon, island as island_surface, label_text, metrics, panel_toggle_icon,
    placeholder_icon, rotating_gear, row_leading, set_chrome_palette, text, theme_snapshot, tokens,
    tokens_from,
};
pub use widgets::{
    ListClickHandler, ListKeyEffect, ListKeyIntent, ListKeyboard, ListRowChrome, ListRowSpec,
    TreeClickHandler, TreeContextMenuBuilder, TreeRowAccessory, TreeRowChrome, TreeRowSpec,
    list_row_chrome, render_list, render_list_row, render_tree_list, render_tree_row,
    step_list_index, tree_guides, tree_row_chrome,
};
