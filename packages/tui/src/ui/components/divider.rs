//! Theme-aware paint adapter for a prepared local divider.

use piko_tui_layout::{DividerPlan, SplitAxis};
use ratatui::{
    Frame,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::theme::Theme;

/// Paint a passive divider.  The plan owns geometry; this adapter owns only
/// glyph and color policy.
#[allow(dead_code)] // public paint adapter; no current product surface uses dividers
pub fn paint_divider(frame: &mut Frame<'_>, plan: &DividerPlan, theme: &Theme) {
    let Some(area) = plan.divider else {
        return;
    };
    let axis = if area.width <= area.height {
        SplitAxis::Horizontal
    } else {
        SplitAxis::Vertical
    };
    paint_divider_with_axis(frame, area, axis, theme);
}

/// Paint when the caller wants to disambiguate a square divider band.
#[allow(dead_code)] // public paint adapter for future divider consumers
pub fn paint_divider_with_axis(
    frame: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    axis: SplitAxis,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let glyph = match axis {
        SplitAxis::Horizontal => "│",
        SplitAxis::Vertical => "─",
    };
    let lines = match axis {
        SplitAxis::Horizontal => (0..area.height)
            .map(|_| Line::from(Span::styled(glyph, Style::default().fg(theme.border_muted))))
            .collect(),
        SplitAxis::Vertical => vec![Line::from(Span::styled(
            glyph.repeat(usize::from(area.width)),
            Style::default().fg(theme.border_muted),
        ))],
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// Compatibility spelling for callers that use render terminology.
#[allow(unused_imports)]
pub use paint_divider as render;
