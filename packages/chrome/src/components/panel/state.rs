//! Island content states: Ready / Loading / Empty / Custom.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::theme::{ChromeIcon, TextRole, placeholder_icon, text, tokens};

/// Optional mark above placeholder title.
pub enum IslandMedia {
    /// Image or fully custom mark (Empty / Loading icons).
    Element(AnyElement),
}

/// Structured centered placeholder for Loading / Empty.
pub struct IslandPlaceholder {
    pub media: Option<IslandMedia>,
    pub title: SharedString,
    pub subtitle: Option<SharedString>,
    pub action: Option<AnyElement>,
}

impl IslandPlaceholder {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            media: None,
            title: title.into(),
            subtitle: None,
            action: None,
        }
    }

    pub fn chrome_icon(mut self, name: ChromeIcon) -> Self {
        let color = crate::theme::ChromeTokens::hsla(tokens().muted_fg);
        self.media = Some(IslandMedia::Element(
            placeholder_icon(name, color).into_any_element(),
        ));
        self
    }

    #[allow(dead_code)]
    pub fn media_element(mut self, element: impl IntoElement) -> Self {
        self.media = Some(IslandMedia::Element(element.into_any_element()));
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    #[allow(dead_code)]
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }
}

/// Content area of an [`super::IslandPanel`].
pub enum IslandBody {
    Ready(AnyElement),
    Loading(IslandPlaceholder),
    Empty(IslandPlaceholder),
    /// Full override of the content area.
    #[allow(dead_code)]
    Custom(AnyElement),
}

impl IslandBody {
    pub fn ready(body: impl IntoElement) -> Self {
        Self::Ready(body.into_any_element())
    }

    pub fn loading(placeholder: IslandPlaceholder) -> Self {
        Self::Loading(placeholder)
    }

    pub fn empty(placeholder: IslandPlaceholder) -> Self {
        Self::Empty(placeholder)
    }

    #[allow(dead_code)]
    pub fn custom(body: impl IntoElement) -> Self {
        Self::Custom(body.into_any_element())
    }

    pub fn uses_scroll_viewport(&self) -> bool {
        matches!(self, Self::Ready(_))
    }
}

/// Shared centered placeholder used by Loading and Empty.
pub fn render_island_placeholder(placeholder: IslandPlaceholder) -> impl IntoElement {
    let t = tokens();
    let m = crate::theme::metrics();
    div()
        .id("island-placeholder")
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(m.space_sm)
        .px(m.space_md)
        .when_some(placeholder.media, |d, media| {
            d.child(match media {
                IslandMedia::Element(el) => el,
            })
        })
        .child(
            text(TextRole::PlaceholderTitle)
                .text_color(t.fg_rgba())
                .text_center()
                .child(placeholder.title),
        )
        .when_some(placeholder.subtitle, |d, subtitle| {
            d.child(
                text(TextRole::PlaceholderSubtitle)
                    .text_color(t.muted_fg_rgba())
                    .text_center()
                    .child(subtitle),
            )
        })
        .when_some(placeholder.action, |d, action| d.child(action))
}
