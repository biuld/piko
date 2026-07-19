//! Shared density and type-scale numbers for the desktop Workbench.
//!
//! Part of [`crate::theme`]. Islands must not hard-code px sizes for text;
//! go through [`crate::theme::typography`] (or [`crate::theme::icons`] for
//! icon boxes). Spacing and layout constants are read via [`metrics`].

use gpui::{Pixels, px};

/// One compact scale shared by every first-class Workbench surface.
#[derive(Debug, Clone, Copy)]
pub struct UiMetrics {
    pub space_xs: Pixels,
    pub space_sm: Pixels,
    pub space_md: Pixels,
    pub space_lg: Pixels,
    pub panel_header_height: Pixels,
    /// Distance from island top to vertically-centered header title
    /// (`(panel_header_height - label_line_height) / 2`).
    pub panel_header_title_inset: Pixels,
    /// Shared left/right inset for tool-window headers and tree rows.
    pub tool_row_inset: Pixels,
    /// Fixed width for the Meta-or-Action accessory rail (header + tree).
    pub tool_accessory_width: Pixels,
    /// Fixed disclosure rail before the terminal accessory (chevron or spacer).
    pub tool_disclosure_width: Pixels,
    /// Reserved for compact chrome rows (secondary strips, etc.).
    #[allow(dead_code)]
    pub compact_bar_height: Pixels,
    pub title_bar_height: Pixels,
    pub status_bar_height: Pixels,
    pub title_bar_safe_inset: Pixels,
    pub chrome_horizontal_padding: Pixels,
    pub status_content_offset_y: Pixels,
    pub island_gutter: Pixels,
    pub island_radius: Pixels,
    pub reading_width: Pixels,
    pub meta_size: Pixels,
    pub meta_line_height: Pixels,
    pub label_size: Pixels,
    pub label_line_height: Pixels,
    pub body_size: Pixels,
    pub body_line_height: Pixels,
}

impl UiMetrics {
    pub const fn compact() -> Self {
        Self {
            space_xs: px(4.),
            space_sm: px(8.),
            space_md: px(12.),
            space_lg: px(16.),
            panel_header_height: px(40.),
            panel_header_title_inset: px(11.),
            tool_row_inset: px(8.),
            tool_accessory_width: px(24.),
            tool_disclosure_width: px(16.),
            compact_bar_height: px(28.),
            title_bar_height: px(34.),
            status_bar_height: px(28.),
            title_bar_safe_inset: px(80.),
            chrome_horizontal_padding: px(8.),
            status_content_offset_y: px(-1.),
            island_gutter: px(8.),
            island_radius: px(10.),
            reading_width: px(880.),
            meta_size: px(12.),
            meta_line_height: px(16.),
            label_size: px(13.),
            label_line_height: px(18.),
            body_size: px(14.),
            body_line_height: px(21.),
        }
    }
}

pub const fn metrics() -> UiMetrics {
    UiMetrics::compact()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_scale_is_stable() {
        let m = metrics();
        assert_eq!(m.panel_header_height, px(40.));
        assert_eq!(m.panel_header_title_inset, px(11.));
        assert_eq!(m.tool_row_inset, px(8.));
        assert_eq!(m.tool_accessory_width, px(24.));
        assert_eq!(m.tool_disclosure_width, px(16.));
        assert_eq!(m.compact_bar_height, px(28.));
        assert_eq!(m.title_bar_height, px(34.));
        assert_eq!(m.status_bar_height, px(28.));
        assert_eq!(m.title_bar_safe_inset, px(80.));
        assert_eq!(m.status_content_offset_y, px(-1.));
        assert_eq!(m.body_size, px(14.));
        assert_eq!(m.body_line_height, px(21.));
        assert_eq!(m.island_gutter, px(8.));
        assert_eq!(m.island_radius, px(10.));
    }
}
