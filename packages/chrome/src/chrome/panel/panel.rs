//! Tool-window IslandPanel: optional header + content states + scroll.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::theme::{island, metrics, tokens};

use super::state::{IslandBody, IslandPlaceholder, render_island_placeholder};
use super::viewport::IslandContentViewport;
/// Header for tool-window islands (title + optional trailing actions).
pub enum IslandHeader {
    Title {
        title: SharedString,
        action: Option<AnyElement>,
    },
}

impl IslandHeader {
    pub fn title(title: impl Into<SharedString>) -> Self {
        Self::Title {
            title: title.into(),
            action: None,
        }
    }

    pub fn title_with_action(title: impl Into<SharedString>, action: AnyElement) -> Self {
        Self::Title {
            title: title.into(),
            action: Some(action),
        }
    }
}

/// First-class workbench island. Header and content scrolling are opt-in so
/// Document-style islands can reuse the shell without tool-window chrome.
#[derive(IntoElement)]
pub struct IslandPanel {
    id: SharedString,
    header: Option<IslandHeader>,
    body: IslandBody,
    /// When true, wrap Ready content in [`IslandContentViewport`]. Loading /
    /// Empty / Custom never use the list scrollbar.
    scroll: bool,
    /// Optional host-owned scroll handle (Timeline follow). When absent,
    /// the panel allocates a keyed handle for tool-window islands.
    scroll_handle: Option<ScrollHandle>,
    /// When true, fill the parent (`size_full`). Intrinsic-height islands use `false` for
    /// intrinsic height.
    fill: bool,
    /// Keyboard focus owner ring (theme `ring`), independent of selection.
    focused: bool,
}

impl IslandPanel {
    /// Create a panel with ready content (most common path).
    pub fn new(id: impl Into<SharedString>, body: impl IntoElement) -> Self {
        Self::with_body(id, IslandBody::ready(body))
    }

    pub fn with_body(id: impl Into<SharedString>, body: IslandBody) -> Self {
        Self {
            id: id.into(),
            header: None,
            body,
            scroll: true,
            scroll_handle: None,
            fill: true,
            focused: false,
        }
    }

    pub fn loading(id: impl Into<SharedString>, placeholder: IslandPlaceholder) -> Self {
        Self::with_body(id, IslandBody::loading(placeholder)).scroll(false)
    }

    pub fn empty(id: impl Into<SharedString>, placeholder: IslandPlaceholder) -> Self {
        Self::with_body(id, IslandBody::empty(placeholder)).scroll(false)
    }

    #[allow(dead_code)]
    pub fn custom(id: impl Into<SharedString>, body: impl IntoElement) -> Self {
        Self::with_body(id, IslandBody::custom(body)).scroll(false)
    }

    #[allow(dead_code)]
    pub fn content(mut self, body: IslandBody) -> Self {
        if !body.uses_scroll_viewport() {
            self.scroll = false;
        }
        self.body = body;
        self
    }

    pub fn header(mut self, header: IslandHeader) -> Self {
        self.header = Some(header);
        self
    }

    pub fn scroll(mut self, enabled: bool) -> Self {
        self.scroll = enabled;
        self
    }

    /// Use a caller-owned [`ScrollHandle`] (e.g. Timeline follow state).
    pub fn scroll_handle(mut self, handle: ScrollHandle) -> Self {
        self.scroll_handle = Some(handle);
        self.scroll = true;
        self
    }

    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl RenderOnce for IslandPanel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scroll = self.scroll && self.body.uses_scroll_viewport();
        let content = match self.body {
            IslandBody::Ready(body) => {
                if scroll {
                    let scroll_handle = self.scroll_handle.unwrap_or_else(|| {
                        let scroll_key = SharedString::from(format!("{}-scroll-handle", self.id));
                        window
                            .use_keyed_state(scroll_key, cx, |_, _| ScrollHandle::new())
                            .read(cx)
                            .clone()
                    });
                    IslandContentViewport::new(
                        SharedString::from(format!("{}-viewport", self.id)),
                        scroll_handle,
                        body,
                    )
                    .into_any_element()
                } else if self.fill {
                    div()
                        .id(SharedString::from(format!("{}-body", self.id)))
                        .flex_1()
                        .min_h(px(0.))
                        .min_w(px(0.))
                        .w_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(body)
                        .into_any_element()
                } else {
                    body
                }
            }
            IslandBody::Loading(placeholder) | IslandBody::Empty(placeholder) => div()
                .id(SharedString::from(format!("{}-placeholder", self.id)))
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .child(render_island_placeholder(placeholder))
                .into_any_element(),
            IslandBody::Custom(body) => {
                if self.fill {
                    div()
                        .id(SharedString::from(format!("{}-custom", self.id)))
                        .flex_1()
                        .min_h(px(0.))
                        .w_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(body)
                        .into_any_element()
                } else {
                    body
                }
            }
        };

        island()
            .id(self.id)
            .w_full()
            .when(self.fill, |d| d.h_full())
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(self.focused, |d| {
                d.border_1().border_color(tokens().ring_rgba())
            })
            .when_some(self.header, |d, header| d.child(render_header(header)))
            .child(content)
    }
}

fn render_header(header: IslandHeader) -> impl IntoElement {
    let m = metrics();
    div()
        .h(m.panel_header_height)
        .w_full()
        .flex_shrink_0()
        // Match IslandContentViewport's outer content gutter. The nested
        // row inset then places header and tree actions on the same rail.
        .px(m.space_xs)
        .flex()
        .items_center()
        .child(div().w_full().px(m.tool_row_inset).child(match header {
            IslandHeader::Title { title, action } => render_title_header(title, action),
        }))
}

fn render_title_header(title: SharedString, action: Option<AnyElement>) -> impl IntoElement {
    let m = metrics();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(m.space_xs)
        .child(
            crate::theme::label_text(true)
                .min_w_0()
                .flex_1()
                .truncate()
                .child(title),
        )
        .child(
            // Header has no disclosure, but reserves the shared rail before
            // its terminal accessory.
            div().w(m.tool_disclosure_width).h_full().flex_shrink_0(),
        )
        .child(
            div()
                .w(m.tool_accessory_width)
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .children(action),
        )
}
