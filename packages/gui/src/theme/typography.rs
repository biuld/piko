//! Named text styles built on [`super::metrics`].
//!
//! All chrome and conversation text sizing goes through this module. Call sites
//! must not apply `UiMetrics` font sizes directly — use [`text`], [`label_text`],
//! or [`body_markdown`].

use std::sync::Arc;

use gpui::{App, Div, ElementId, FontWeight, SharedString, Styled, Window, div, rems};
use gpui_component::highlighter::HighlightTheme;
use gpui_component::text::{TextView, TextViewStyle};

use super::metrics::metrics;
use super::tokens::tokens;

/// Chrome / conversation text roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRole {
    Meta,
    Label,
    Body,
    BodyMono,
    PlaceholderTitle,
    PlaceholderSubtitle,
}

impl TextRole {
    pub fn apply(self, el: Div) -> Div {
        let m = metrics();
        match self {
            Self::Meta => el.text_size(m.meta_size).line_height(m.meta_line_height),
            Self::Label => el.text_size(m.label_size).line_height(m.label_line_height),
            Self::Body | Self::PlaceholderSubtitle => {
                el.text_size(m.body_size).line_height(m.body_line_height)
            }
            Self::BodyMono => el
                .text_size(m.body_size)
                .line_height(m.body_line_height)
                .font_family("monospace"),
            Self::PlaceholderTitle => el
                .text_size(m.label_size)
                .line_height(m.label_line_height)
                .font_weight(FontWeight::SEMIBOLD),
        }
    }
}

/// Apply a text role to a fresh `div()`.
pub fn text(role: TextRole) -> Div {
    role.apply(div())
}

/// Convenience: label row that may be semibold when selected/emphasized.
pub fn label_text(semibold: bool) -> Div {
    let el = TextRole::Label.apply(div());
    if semibold {
        el.font_weight(FontWeight::SEMIBOLD)
    } else {
        el
    }
}

/// [`TextView`] style pinned to the Body type scale and 12 px vertical rhythm.
///
/// Conversation markdown should read as a document: modest heading steps,
/// dark syntax theme, and paragraph gaps aligned with [`metrics`]`::space_md`.
pub fn markdown_style() -> TextViewStyle {
    let m = metrics();
    // paragraph_gap is Rems; 0.75 rem ≈ 12 px when rem size is 16 (space_md).
    TextViewStyle {
        heading_base_font_size: m.body_size,
        paragraph_gap: rems(0.75),
        heading_font_size: Some(Arc::new(|level, base| match level {
            1 => base * 1.2,
            2 => base * 1.12,
            3 => base * 1.06,
            _ => base,
        })),
        highlight_theme: HighlightTheme::default_dark(),
        is_dark: true,
        ..TextViewStyle::default()
    }
}

/// Markdown [`TextView`] already styled as conversation Body (14 / 21 + theme fg).
///
/// This is the TextView counterpart of [`text`]`(`[`TextRole::Body`]`)`. GPUI
/// Component has no TextRole API, so Body metrics are applied here once.
///
/// Call sites should place the result in a `w_full` container so list item
/// text is not clipped by nested `overflow_hidden` flex rows inside TextView.
pub fn body_markdown(
    id: impl Into<ElementId>,
    markdown: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> TextView {
    let m = metrics();
    TextView::markdown(id, markdown, window, cx)
        .style(markdown_style())
        .w_full()
        .text_size(m.body_size)
        .line_height(m.body_line_height)
        .text_color(tokens().fg_rgba())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn roles_match_compact_metrics() {
        let m = metrics();
        assert_eq!(m.meta_size, px(12.));
        assert_eq!(m.label_size, px(13.));
        assert_eq!(m.body_size, px(14.));
        let style = markdown_style();
        assert_eq!(style.heading_base_font_size, m.body_size);
        assert!(style.is_dark);
        assert_eq!(style.paragraph_gap, rems(0.75));
        let _ = text(TextRole::Meta);
        let _ = text(TextRole::Body);
        let _ = text(TextRole::BodyMono);
    }

    #[test]
    fn conversation_heading_scale_stays_modest() {
        let style = markdown_style();
        let f = style.heading_font_size.as_ref().expect("heading fn");
        let base = px(14.);
        assert_eq!(f(1, base), base * 1.2);
        assert_eq!(f(2, base), base * 1.12);
        assert_eq!(f(6, base), base);
    }
}
