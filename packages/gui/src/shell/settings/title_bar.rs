//! Settings TitleBar — brand + gear toggle (no Back). Same trailing alignment as Workbench.

use gpui::*;
use gpui_component::TitleBar;

use crate::app::desktop_app::DesktopApp;
use crate::shell::workbench::title_bar::{notification_bell, settings_gear};
use crate::theme::{label_text, metrics, tokens};

pub fn render_title_bar(
    notifications_open: bool,
    notifications_unread: bool,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();

    TitleBar::new().h(m.title_bar_height).child(
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .absolute()
                    .right(m.island_gutter)
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .gap(m.space_xs)
                    .child(notification_bell(
                        notifications_open,
                        notifications_unread,
                        entity.clone(),
                    ))
                    .child(settings_gear(true, entity)),
            )
            .child(
                label_text(false)
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(m.space_xs)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.fg_rgba())
                    .child("piko")
                    .child(
                        text(crate::theme::TextRole::Label)
                            .text_color(t.muted_fg_rgba())
                            .font_weight(FontWeight::NORMAL)
                            .child(format!("/ {}", crate::t!("settings.title"))),
                    ),
            ),
    )
}

use crate::theme::text;
