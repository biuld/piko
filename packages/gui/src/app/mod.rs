//! GPUI application root: DesktopApp, actions, host wiring, persist.

mod composer_host;
pub mod desktop_app;
mod island_refresh;
pub mod layout_state;
pub mod model_cycle;
mod notifications;
pub mod primary_surface;
pub(crate) mod quit;
pub(crate) mod quit_busy;
mod session_prefs;
mod submit_recovery;
pub mod timeline_follow;
pub mod ux_prefs;
pub(crate) mod wiring;

pub(crate) use wiring::island_actions;
pub(crate) use wiring::island_dispatch;
