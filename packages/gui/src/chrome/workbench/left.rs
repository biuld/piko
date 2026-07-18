//! Left column: Sessions island slot.

use gpui::*;

use crate::islands::SessionsIsland;

/// Horizontal gutter slot for the Sessions dock column.
pub(crate) fn render_left_column(
    sessions: Entity<SessionsIsland>,
    trailing_gutter: Pixels,
) -> impl IntoElement {
    column_slot(sessions, px(0.), trailing_gutter)
}

pub(crate) fn column_slot(
    child: impl IntoElement,
    leading: Pixels,
    trailing_gutter: Pixels,
) -> impl IntoElement {
    div()
        .size_full()
        .min_h(px(0.))
        .pl(leading)
        .pr(trailing_gutter)
        .overflow_hidden()
        .child(child)
}
