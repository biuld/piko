use crate::theme::Theme;
use crate::ui::line_layout::{
    DEFAULT_EDGE_INSET, DEFAULT_TRAILING_SPACER, pad_spans, paint_cols, truncate_cols,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub(super) fn scan_row(
    width: u16,
    selected: bool,
    theme: &Theme,
    left: Vec<(String, Color)>,
    right: Option<(&str, Color)>,
) -> Line<'static> {
    let fill = if selected {
        Style::default().bg(theme.bg_selected)
    } else {
        Style::default()
    };
    let marker = if selected { "› " } else { "  " };
    let mut marker_style = fill.fg(if selected { theme.accent } else { theme.dim });
    if selected {
        marker_style = marker_style.add_modifier(Modifier::BOLD);
    }
    let target = usize::from(width);
    let right = right.filter(|(text, _)| {
        // Metadata is optional; keep room for a meaningful primary summary.
        usize::from(width) >= paint_cols(text) + 38
    });
    let right_text = right.map(|(text, _)| text).unwrap_or("");
    let right_style = right
        .map(|(_, color)| fill.fg(color))
        .unwrap_or(fill.fg(Color::Reset));
    let reserve = if right_text.is_empty() {
        DEFAULT_EDGE_INSET
    } else {
        paint_cols(right_text) + DEFAULT_TRAILING_SPACER + DEFAULT_EDGE_INSET
    };
    let mut budget = target
        .saturating_sub(paint_cols(marker))
        .saturating_sub(reserve);
    let mut spans = vec![Span::styled(marker.to_string(), marker_style)];
    for (text, color) in left {
        if budget == 0 {
            break;
        }
        let style = if selected {
            fill.fg(color).add_modifier(Modifier::BOLD)
        } else {
            fill.fg(color)
        };
        let fitted = truncate_cols(&text, budget);
        budget = budget.saturating_sub(paint_cols(&fitted));
        if fitted.is_empty() {
            continue;
        }
        spans.push(Span::styled(fitted, style));
    }
    if !right_text.is_empty() {
        let used: usize = spans
            .iter()
            .map(|span| paint_cols(span.content.as_ref()))
            .sum();
        let gap = target
            .saturating_sub(used)
            .saturating_sub(paint_cols(right_text))
            .saturating_sub(DEFAULT_EDGE_INSET);
        if gap > 0 {
            spans.push(Span::styled(" ".repeat(gap), fill));
        }
        spans.push(Span::styled(right_text.to_string(), right_style));
    }
    pad_spans(spans, fill, width)
}

pub(super) fn kv(key: &str, value: impl Into<String>, theme: &Theme, width: u16) -> Line<'static> {
    plain(format!("{key}  {}", value.into()), theme.text, width)
}

pub(super) fn plain(text: impl Into<String>, color: Color, width: u16) -> Line<'static> {
    let text = truncate_cols(&text.into(), usize::from(width));
    pad_spans(
        vec![Span::styled(text, Style::default().fg(color))],
        Style::default(),
        width,
    )
}

pub(in crate::features::history) fn wrapped(
    text: &str,
    color: Color,
    width: u16,
) -> Vec<Line<'static>> {
    crate::ui::line_layout::soft_wrap(text, usize::from(width.max(1)))
        .into_iter()
        .map(|line| plain(line, color, width))
        .collect()
}
