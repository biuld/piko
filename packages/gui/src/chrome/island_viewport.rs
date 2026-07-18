//! Shared island content viewport: themed gpui-component scrollbar.

use gpui::*;
use gpui_component::scroll::ScrollableElement;

use crate::theme::metrics;

/// Scroll body used by Sessions, Timeline, Agents, and Tree via IslandPanel.
///
/// Vertical scrollbar only. Thumbs are overflow-gated by gpui-component (no
/// paint when content fits the viewport). Padding lives on the scroll child,
/// not the overflow element, so short content does not look permanently
/// scrollable.
#[derive(IntoElement)]
pub struct IslandContentViewport {
    id: SharedString,
    scroll: ScrollHandle,
    body: AnyElement,
}

impl IslandContentViewport {
    pub fn new(id: impl Into<SharedString>, scroll: ScrollHandle, body: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            scroll,
            body: body.into_any_element(),
        }
    }
}

impl RenderOnce for IslandContentViewport {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let m = metrics();
        let scroll_id = SharedString::from(format!("{}-scroll", self.id));

        div()
            .id(self.id)
            .flex_1()
            .min_h(px(0.))
            .min_w(px(0.))
            .w_full()
            .relative()
            .overflow_hidden()
            .child(
                div()
                    .id(scroll_id)
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .w_full()
                            .px(m.space_xs)
                            .py(m.space_xs)
                            .child(self.body),
                    ),
            )
            .vertical_scrollbar(&self.scroll)
    }
}
