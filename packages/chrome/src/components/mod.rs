//! GPUI composite components: panels, overlays, notifications, and list/tree widgets.

pub mod list;
pub mod markdown;
pub mod menu;
pub mod notification;
pub mod overlay;
pub mod panel;
pub mod selection;

/// Register key bindings used by stateful chrome components.
pub fn init(cx: &mut gpui::App) {
    menu::init(cx);
    selection::init(cx);
}
