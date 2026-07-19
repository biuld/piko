//! Quit-on-close confirm dialog (GPUI).

use gpui::*;
use gpui_component::WindowExt;
use gpui_component::button::ButtonVariant;
use gpui_component::dialog::DialogButtonProps;

pub use super::quit_busy::is_quit_busy;

/// Open the quit confirm dialog; OK calls `cx.quit()`.
pub fn open_quit_confirm(window: &mut Window, cx: &mut App) {
    let title = crate::t!("dialog.quit.title");
    let body = crate::t!("dialog.quit.body");
    let quit_label = crate::t!("dialog.quit.confirm");
    let cancel_label = crate::t!("dialog.action.cancel");

    window.open_dialog(cx, move |dialog, _window, _cx| {
        dialog
            .title(title.clone())
            .child(div().text_sm().child(body.clone()))
            .confirm()
            .button_props(
                DialogButtonProps::default()
                    .ok_text(quit_label.clone())
                    .ok_variant(ButtonVariant::Danger)
                    .cancel_text(cancel_label.clone()),
            )
            .on_ok(|_, _window, cx| {
                cx.quit();
                true
            })
    });
}
