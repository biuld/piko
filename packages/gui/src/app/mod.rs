//! GPUI application root: DesktopApp, actions, host wiring, persist.

pub mod archipelago;
mod composer_host;
pub mod desktop_app;
mod island_refresh;
pub mod layout_state;
pub mod model_cycle;
mod notifications;
pub(crate) mod quit;
pub(crate) mod quit_busy;
mod session_prefs;
mod submit_recovery;
pub mod timeline_follow;
pub mod ux_prefs;
pub(crate) mod wiring;

pub(crate) use wiring::island_actions;
pub(crate) use wiring::island_dispatch;
