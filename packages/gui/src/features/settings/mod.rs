//! Settings product feature — forms, nav, and host/gui mutations.
//!
//! Shell owns the Settings Primary Surface frame; this module owns section IA
//! and panel content.

mod actions;
mod nav;
mod render;
mod section;
mod sections;
mod widgets;

pub use nav::render_nav;
pub use render::render_panel;
pub use section::SettingsSection;
