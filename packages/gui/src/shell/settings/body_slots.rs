//! Settings body: horizontal Nav | Panel island workspaces.

use gpui::*;

use crate::theme::metrics;

/// Lay out the two Settings islands (already `IslandPanel` chrome + focus ring).
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
        .child(
            div()
                .id("settings-nav-slot")
                .w(px(220.))
                .flex_shrink_0()
                .h_full()
                .min_h(px(0.))
                .child(nav),
        )
        .child(
            div()
                .id("settings-panel-slot")
                .flex_1()
                .min_w(px(0.))
                .h_full()
                .min_h(px(0.))
                .child(panel),
        )
}
