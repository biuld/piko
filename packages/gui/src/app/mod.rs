//! GPUI application root: DesktopApp, actions, host wiring, persist.

mod composer_host;
pub mod desktop_app;
pub(crate) mod island_actions;
pub(crate) mod island_dispatch;
mod island_refresh;
pub mod layout_state;
pub mod model_cycle;
mod notifications;
mod overlay_host;
mod palette_actions;
mod primary_surface_nav;
pub(crate) mod quit;
pub(crate) mod quit_busy;
mod settings_actions;
mod submit_recovery;
pub mod timeline_follow;
pub mod ux_prefs;
