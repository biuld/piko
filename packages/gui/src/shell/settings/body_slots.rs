//! Settings body slots — nav island + panel island chrome (no section forms).

use gpui::*;

use crate::theme::{island, metrics};

pub fn body_slots(nav: impl IntoElement, panel: impl IntoElement) -> impl IntoElement {
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
        .child(render_nav_island(nav))
        .child(render_panel_island(panel))
}

fn render_nav_island(nav: impl IntoElement) -> impl IntoElement {
    island()
        .id("settings-nav-island")
        .w(px(220.))
        .flex_shrink_0()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(nav)
}

fn render_panel_island(panel: impl IntoElement) -> impl IntoElement {
    island()
        .id("settings-panel-island")
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(panel)
}
