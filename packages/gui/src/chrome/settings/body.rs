//! Settings body — island nav + island panel (Fleet canvas gutters).

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::chrome::primary_surface::SettingsSection;
use crate::chrome::settings::{render_nav, sections};
use crate::theme::{island, label_text, metrics, tokens};

pub fn render_body(
    section: SettingsSection,
    app: &DesktopApp,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let m = metrics();

    div()
        .id("settings-body")
        .flex_1()
        .min_h(px(0.))
        .overflow_hidden()
        .p(m.island_gutter)
        .flex()
        .flex_row()
        .gap(m.island_gutter)
        .child(render_nav_island(section, entity.clone()))
        .child(render_panel_island(section, app, entity))
}

fn render_nav_island(section: SettingsSection, entity: WeakEntity<DesktopApp>) -> impl IntoElement {
    island()
        .id("settings-nav-island")
        .w(px(220.))
        .flex_shrink_0()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(render_nav(section, entity))
}

fn render_panel_island(
    section: SettingsSection,
    app: &DesktopApp,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();

    island()
        .id("settings-panel-island")
        .flex_1()
        .min_w(px(0.))
        .h_full()
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
                        .child(sections::render_section_panel(section, app, entity)),
                ),
        )
}
