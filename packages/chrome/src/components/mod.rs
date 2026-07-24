//! GPUI composite components: island panel shell, overlays, list/tree widgets.

pub mod list;
pub mod markdown;
pub mod menu;
pub mod overlay;
pub mod panel;
pub mod selection;

/// Register key bindings used by stateful chrome components.
pub fn init(cx: &mut gpui::App) {
    menu::init(cx);
    selection::init(cx);
}
