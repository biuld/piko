//! Settings panel body — section header + scrollable form content.

use gpui::*;

use crate::app::desktop_app::DesktopApp;

use super::SettingsSection;
use super::sections::render_section_panel;
use crate::theme::{label_text, metrics, tokens};

pub fn render_panel(
    section: SettingsSection,
    app: &DesktopApp,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();

    div()
        .id("settings-panel-inner")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .h(m.panel_header_height)
                .px(m.tool_row_inset)
                .flex()
                .items_center()
                .flex_shrink_0()
                .child(
                    label_text(true)
                        .text_color(t.fg_rgba())
                        .child(crate::t!(section.i18n_key())),
                ),
        )
        .child(
            div()
                .id("settings-panel")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .px(m.tool_row_inset)
                .pb(m.space_lg)
                .flex()
                .flex_col()
                .child(
                    div()
                        .w_full()
                        .max_w(m.reading_width)
                        .flex()
                        .flex_col()
                        .gap(m.space_md)
                        .child(render_section_panel(section, app, entity)),
                ),
        )
}
