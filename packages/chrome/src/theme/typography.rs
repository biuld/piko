//! Named text styles built on [`super::metrics`].
//!
//! All chrome and conversation text sizing goes through this module. Call sites
//! must not apply `UiMetrics` font sizes directly — use [`text`] or
//! [`label_text`]. Native Markdown uses the same roles internally.

use gpui::{Div, FontWeight, Styled, div};

use super::metrics::metrics;

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
        let _ = text(TextRole::Meta);
        let _ = text(TextRole::Body);
        let _ = text(TextRole::BodyMono);
    }
}
