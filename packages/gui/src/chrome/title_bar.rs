//! Native-integrated custom title bar for the desktop window chrome.

use gpui::*;
use gpui_component::TitleBar;

use crate::theme::{label_text, metrics, tokens};
use piko_client_core::ClientState;

pub fn render_title_bar(state: &ClientState, project_name: &str) -> impl IntoElement {
    let m = metrics();
    let t = tokens();
    let context = state
        .live_session
        .as_ref()
        .and_then(|session| session.name.clone())
        .unwrap_or_else(|| project_name.to_string());

    TitleBar::new().h(m.title_bar_height).child(
        label_text(false)
            .size_full()
            .pr(m.title_bar_safe_inset)
            .flex()
            .items_center()
            .justify_center()
            .gap(m.space_sm)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.fg_rgba())
                    .child("piko"),
            )
            .child(div().text_color(t.muted_fg_rgba()).child("/"))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(t.muted_fg_rgba())
                    .child(context),
            ),
    )
}
