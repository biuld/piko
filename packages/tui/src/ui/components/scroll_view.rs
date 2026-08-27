//! Paint adapter for prepared styled text and viewport geometry.

use piko_tui_layout::ViewportPlan;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::{
    theme::Theme,
    ui::text_layout::{TextLayout, VisualLine},
};

/// Paint visible rows from a prepared text layout into a prepared viewport.
///
/// This function intentionally does not wrap, scroll, or map owners to
/// actions.  The caller changes `ViewportState`, prepares a new viewport plan
/// when geometry changes, and can reuse the text plan during pure scrolling.
pub fn paint_scroll_view(
    frame: &mut Frame<'_>,
    layout: &TextLayout<Style>,
    viewport: &ViewportPlan,
    theme: &Theme,
) {
    if viewport.content.width == 0 || viewport.content.height == 0 {
        return;
    }
    let first = viewport.visible.start.min(layout.lines.len());
    let last = viewport.visible.end.min(layout.lines.len());
    for (offset, line) in layout.lines[first..last].iter().enumerate() {
        let Some(y) = viewport.content.y.checked_add(offset as u16) else {
            break;
        };
        if y >= viewport.content.bottom() {
            break;
        }
        frame.render_widget(
            line_widget(line),
            Rect::new(viewport.content.x, y, viewport.content.width, 1),
        );
    }

    if let Some(metrics) = viewport.scrollbar {
        let mut state = ScrollbarState::new(metrics.content_rows)
            .position(metrics.content_position())
            .viewport_content_length(metrics.visible_rows.max(1));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .style(Style::default().fg(theme.border_muted))
                .thumb_style(Style::default().fg(theme.dim)),
            viewport.gutter,
            &mut state,
        );
    }
}

fn line_widget(line: &VisualLine<Style>) -> Line<'static> {
    Line::from(
        line.fragments
            .iter()
            .map(|fragment| Span::styled(fragment.text.clone(), fragment.payload))
            .collect::<Vec<_>>(),
    )
}

/// Alias matching the component name used in the design document.
#[allow(unused_imports)]
pub use paint_scroll_view as render;
