//! Terminal **line** layout primitives (column math + ratatui `Line` paint).
//!
//! This is **not** flex region layout (`piko-tui-layout`). It lays out text
//! inside a single row band: left content, optional reserved right zone, soft
//! wrap by display columns.
//!
//! ```text
//! | left (wraps in this column)     | sp | trailing |
//! | continuation lines same width   |    |          |
//! ```
//!
//! Consumers: timeline messages (timestamp), tool titles (status chips), and
//! any other one-row left/right chrome.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

// ── Column measurement ───────────────────────────────────────────────────────
//
// All widths come from the `unicode-width` crate (Wide/Fullwidth → 2, else the
// crate's default for Ambiguous/Neutral). No locale-specific “non-ASCII ≥ 2”
// override — that was over-conservative and wrong for many symbols.

/// Display width via [`unicode_width::UnicodeWidthStr`].
pub fn paint_cols(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

/// Take a prefix of `text` that fits in `max_cols` columns.
/// Returns `(prefix, remainder)`.
pub fn take_prefix_cols(text: &str, max_cols: usize) -> (String, &str) {
    use unicode_width::UnicodeWidthChar;

    let mut cols = 0usize;
    let mut end = 0usize;
    for (i, ch) in text.char_indices() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w == 0 {
            end = i + ch.len_utf8();
            continue;
        }
        if cols.saturating_add(w) > max_cols {
            break;
        }
        cols += w;
        end = i + ch.len_utf8();
    }
    (text[..end].to_string(), &text[end..])
}

/// Soft-wrap `text` to at most `max_cols` columns per line.
/// Hard newlines are preserved; empty hard lines yield empty rows.
pub fn soft_wrap(text: &str, max_cols: usize) -> Vec<String> {
    let max_cols = max_cols.max(1);
    let mut out = Vec::new();
    for hard in text.split('\n') {
        if hard.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut rest = hard;
        while !rest.is_empty() {
            let (chunk, next) = take_prefix_cols(rest, max_cols);
            if chunk.is_empty() {
                // Pathological: double-width char into a 1-col budget.
                let mut chars = rest.chars();
                let ch = chars.next().expect("rest non-empty");
                out.push(ch.to_string());
                rest = chars.as_str();
                continue;
            }
            out.push(chunk);
            rest = next;
        }
    }
    out
}

/// Truncate to columns (no ellipsis).
pub fn truncate_paint_cols(text: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if paint_cols(text) <= max_cols {
        return text.to_string();
    }
    let (chunk, _) = take_prefix_cols(text, max_cols);
    chunk
}

/// Truncate to columns, suffixing ASCII `...` when clipped.
pub fn truncate_cols(text: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if paint_cols(text) <= max_cols {
        return text.to_string();
    }
    let ellipsis = "...";
    let ellipsis_w = 3usize;
    if max_cols <= ellipsis_w {
        return ellipsis.chars().take(max_cols).collect();
    }
    let keep = max_cols - ellipsis_w;
    let (chunk, _) = take_prefix_cols(text, keep);
    format!("{chunk}{ellipsis}")
}

// ── Row paint ────────────────────────────────────────────────────────────────

/// Fill a single style run to exact `width` paint columns (clip then pad).
pub fn filled_line(text: impl Into<String>, style: Style, width: u16) -> Line<'static> {
    use unicode_width::UnicodeWidthChar;

    let target = usize::from(width);
    if target == 0 {
        return Line::from(Span::styled(String::new(), style));
    }

    let raw = text.into();
    let mut out = String::with_capacity(raw.len().saturating_add(target));
    let mut cols = 0usize;
    for ch in raw.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w == 0 {
            out.push(ch);
            continue;
        }
        if cols.saturating_add(w) > target {
            break;
        }
        out.push(ch);
        cols += w;
    }
    if cols < target {
        out.push_str(&" ".repeat(target - cols));
    }
    Line::from(Span::styled(out, style))
}

/// Pad a span list to `width` with `fill` background spaces.
pub fn pad_spans(mut spans: Vec<Span<'static>>, fill: Style, width: u16) -> Line<'static> {
    let used: usize = spans.iter().map(|s| paint_cols(s.content.as_ref())).sum();
    let target = usize::from(width);
    if used < target {
        spans.push(Span::styled(" ".repeat(target - used), fill));
    }
    Line::from(spans)
}

/// Default blank columns between left content and a trailing chip.
pub const DEFAULT_TRAILING_SPACER: usize = 2;
/// Outer margin after a right affix — matches typical left inset (`" …"`).
pub const DEFAULT_EDGE_INSET: usize = 1;

/// Inputs for [`left_right_line`].
pub struct LeftRightLine<'a> {
    pub left: &'a str,
    pub left_style: Style,
    pub right: &'a str,
    pub right_style: Style,
    pub fill: Style,
    pub width: u16,
    /// Gap between left content and right affix.
    pub mid_spacer: usize,
    /// Blank columns after the affix (keep off the band edge).
    pub edge_inset: usize,
}

/// `left …… right` on one row. Right is never truncated; a trailing edge inset
/// keeps the affix off the band edge (same width as left padding by default).
///
/// ```text
/// | left | …… spacer …… | right | edge |
/// ```
pub fn left_right_line(spec: LeftRightLine<'_>) -> Line<'static> {
    let LeftRightLine {
        left,
        left_style,
        right,
        right_style,
        fill,
        width,
        mid_spacer,
        edge_inset,
    } = spec;

    let target = usize::from(width);
    if target == 0 {
        return Line::from("");
    }
    let right_w = paint_cols(right);
    // Reserve mid spacer + affix + edge so left never crowds the clock.
    let left_budget = target
        .saturating_sub(right_w)
        .saturating_sub(mid_spacer)
        .saturating_sub(edge_inset);
    let left_fit = truncate_paint_cols(left, left_budget);
    let left_w = paint_cols(&left_fit);
    // Mid gap absorbs leftover after left + right + edge.
    let spacer = target
        .saturating_sub(left_w)
        .saturating_sub(right_w)
        .saturating_sub(edge_inset);
    let mut spans = vec![Span::styled(left_fit, left_style)];
    if spacer > 0 {
        spans.push(Span::styled(" ".repeat(spacer), fill));
    }
    spans.push(Span::styled(right.to_string(), right_style));
    if edge_inset > 0 {
        spans.push(Span::styled(" ".repeat(edge_inset), fill));
    }
    let used: usize = spans.iter().map(|s| paint_cols(s.content.as_ref())).sum();
    if used < target {
        spans.push(Span::styled(" ".repeat(target - used), fill));
    }
    Line::from(spans)
}

/// Left content + blank reserved right zone (continuation rows).
pub fn left_column_line(
    left: &str,
    left_style: Style,
    fill: Style,
    width: u16,
    right_reserve: usize,
) -> Line<'static> {
    let target = usize::from(width);
    let left_budget = target.saturating_sub(right_reserve);
    let left_fit = truncate_paint_cols(left, left_budget);
    let left_w = paint_cols(&left_fit);
    let pad = target.saturating_sub(left_w);
    let mut spans = vec![Span::styled(left_fit, left_style)];
    if pad > 0 {
        spans.push(Span::styled(" ".repeat(pad), fill));
    }
    Line::from(spans)
}

/// Reserve width for a trailing label:
/// `paint_cols(label) + mid_spacer + edge_inset`.
pub fn trailing_reserve(trailing: Option<&str>, mid_spacer: usize, edge_inset: usize) -> usize {
    trailing
        .map(|t| {
            paint_cols(t)
                .saturating_add(mid_spacer)
                .saturating_add(edge_inset)
        })
        .unwrap_or(0)
}

/// Parameters for [`body_with_trailing`].
pub struct BodyWithTrailing<'a> {
    pub text: &'a str,
    /// Painted on the **first** row only.
    pub trailing: Option<&'a str>,
    /// Right-zone width on **every** row (usually [`trailing_reserve`]).
    pub reserve: usize,
    pub left_style: Style,
    pub trailing_style: Style,
    pub fill: Style,
    pub width: u16,
    pub leading_space: bool,
    /// Pad rows to full width when there is no reserve (e.g. user card bg).
    pub pad_rows: bool,
}

/// Soft-wrap `text` into a left column; optional trailing on row 0.
///
/// ```text
/// | pad | left (wraps here)     | sp | trailing | pad |
/// | pad | continuation          |               | pad |
/// ```
/// Edge inset after the affix matches left padding when `leading_space` is set.
pub fn body_with_trailing(layout: BodyWithTrailing<'_>) -> Vec<Line<'static>> {
    let BodyWithTrailing {
        text,
        trailing,
        reserve,
        left_style,
        trailing_style,
        fill,
        width,
        leading_space,
        pad_rows,
    } = layout;

    let target = usize::from(width);
    if target == 0 {
        return Vec::new();
    }
    let lead = usize::from(leading_space);
    // Mirror left pad on the right when we have a trailing zone.
    let edge = if leading_space { DEFAULT_EDGE_INSET } else { 0 };
    let left_max = target.saturating_sub(lead).saturating_sub(reserve).max(1);

    let wrap_row = |left: String, paint_trailing: Option<&str>| -> Line<'static> {
        if let Some(ts) = paint_trailing {
            left_right_line(LeftRightLine {
                left: &left,
                left_style,
                right: ts,
                right_style: trailing_style,
                fill,
                width,
                mid_spacer: DEFAULT_TRAILING_SPACER,
                edge_inset: edge,
            })
        } else if reserve > 0 {
            left_column_line(&left, left_style, fill, width, reserve)
        } else if pad_rows {
            filled_line(left, left_style, width)
        } else {
            Line::from(Span::styled(left, left_style))
        }
    };

    let wrapped = soft_wrap(text, left_max);
    if wrapped.is_empty() {
        let left = if leading_space {
            " ".to_string()
        } else {
            String::new()
        };
        return vec![wrap_row(left, trailing)];
    }

    wrapped
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            let left = if leading_space {
                format!(" {chunk}")
            } else {
                chunk
            };
            let paint_trailing = if i == 0 { trailing } else { None };
            wrap_row(left, paint_trailing)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    #[test]
    fn soft_wrap_respects_column_budget() {
        let rows = soft_wrap("abcdefghij", 4);
        assert_eq!(rows, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn soft_wrap_keeps_hard_newlines() {
        let rows = soft_wrap("ab\n\ncd", 10);
        assert_eq!(rows, vec!["ab", "", "cd"]);
    }

    #[test]
    fn body_with_trailing_keeps_left_budget_on_continuations() {
        let style = Style::default();
        let lines = body_with_trailing(BodyWithTrailing {
            text: "abcdefghijklmnopqrstuvwxyz",
            trailing: Some("14:32"),
            reserve: trailing_reserve(Some("14:32"), DEFAULT_TRAILING_SPACER, DEFAULT_EDGE_INSET),
            left_style: style,
            trailing_style: style,
            fill: style,
            width: 20,
            leading_space: true,
            pad_rows: true,
        });
        assert!(lines.len() >= 2, "{lines:?}");
        let first: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first.contains("14:32"), "{first}");
        // Affix must not sit flush on the right edge (edge inset).
        assert!(first.ends_with(' '), "right edge inset missing: {first:?}");
        for line in lines.iter().skip(1) {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(!text.contains("14:32"), "no clock on continuation: {text}");
        }
    }

    #[test]
    fn paint_cols_matches_unicode_width() {
        assert_eq!(paint_cols("a"), 1);
        assert_eq!(paint_cols("请"), 2);
        // Neutral check mark — unicode-width reports 1 (not forced to 2).
        assert_eq!(paint_cols("✓"), 1);
    }

    #[test]
    fn truncate_cols_appends_ascii_ellipsis() {
        assert_eq!(truncate_cols("abcdefghij", 7), "abcd...");
        assert_eq!(truncate_cols("short", 10), "short");
    }
}
