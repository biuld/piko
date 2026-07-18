//! Shared Workbench chrome: island shell, fixed layout tree, window bars.
//!
//! Layout units are islands only. Chrome owns the fixed Workbench tree,
//! gutters, resize, directed island messaging, and keyboard focus ownership.

pub mod agents_tree;
pub mod center;
pub mod layout;
pub mod workbench;

mod island_focus;
mod island_msg;
mod island_panel;
mod island_phase;
mod island_state;
#[cfg(test)]
#[path = "island_state_tests.rs"]
mod island_state_tests;
mod island_viewport;
mod status_bar;
mod title_bar;

pub use agents_tree::render_agents_tree_entities;
pub use island_focus::{FocusCycleDir, IslandFocusRing, focus_order};
pub use island_msg::IslandMsg;
pub use island_panel::{IslandHeader, IslandPanel};
pub use island_phase::IslandSessionPhase;
#[allow(unused_imports)] // public override API for islands
pub use island_state::{IslandBody, IslandMedia, IslandPlaceholder};
pub use layout::{IslandId, prune_island_tree, workbench_island_tree};
pub use status_bar::render_status_bar;
pub use title_bar::render_title_bar;
