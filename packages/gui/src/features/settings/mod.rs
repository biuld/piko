//! Settings product feature — forms, nav, and host/gui mutations.
//!
//! Shell owns the Settings Archipelago frame; this module owns section IA,
//! panel content, and **Settings body islands** ([`SettingsNavIsland`] /
//! [`SettingsPanelIsland`]).

mod actions;
mod islands;
mod nav;
mod render;
mod section;
mod sections;
mod widgets;

pub use islands::{SETTINGS_FOCUS_ORDER, SettingsIslandId, SettingsNavIsland, SettingsPanelIsland};
pub use nav::{ConfirmSection, SelectNextSection, SelectPrevSection};
pub use section::SettingsSection;
