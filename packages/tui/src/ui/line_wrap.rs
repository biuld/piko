//! Column-aware wrapping for styled spans and prefixed rows.
//!
//! [`wrap_spans`] splits a styled span list at display-column boundaries while
//! preserving each span's style; [`prefixed_wrap`] keeps a leading prefix on
//! the first row and indents continuation rows, padding every row to width.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::line_layout::{pad_spans, paint_cols, soft_wrap};
use super::text_layout::{to_lines, wrap_spans as prepare_spans};

/// Soft-wrap styled spans to at most `max_cols` columns, preserving span
/// styles and hard newlines. Adjacent spans on the same row keep their own
/// styles; a span wider than the budget is split at column boundaries.
pub fn wrap_spans(spans: Vec<Span<'static>>, max_cols: usize) -> Vec<Line<'static>> {
    to_lines(&prepare_spans(spans, max_cols))
}

/// Lay out a styled prefix plus flowing `text` into `width` columns: the
/// prefix stays on the first row, continuation rows indent to the prefix
/// width, and every row is padded to `width` with `fill`. Used for gutter
/// rows (diff/code) and labeled rows (notice/errors/tool body).
pub fn prefixed_wrap(
    prefix: Vec<Span<'static>>,
    text: &str,
    text_style: Style,
    fill: Style,
    width: u16,
) -> Vec<Line<'static>> {
    let target = usize::from(width);
    if target == 0 {
        return Vec::new();
    }

    let pf: usize = prefix.iter().map(|s| paint_cols(s.content.as_ref())).sum();
    let indent = " ".repeat(pf);
    if pf.saturating_add(1) >= target {
        return vec![pad_spans(prefix, fill, width)];
    }

    let text_budget = target.saturating_sub(pf);
    let chunks = soft_wrap(text, text_budget);
    if chunks.is_empty() {
        return vec![pad_spans(prefix, fill, width)];
    }

    chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut spans = if i == 0 {
                prefix.clone()
            } else {
                vec![Span::styled(indent.clone(), fill)]
            };
            if !chunk.is_empty() {
                spans.push(Span::styled(chunk, text_style));
            }
            pad_spans(spans, fill, width)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    #[test]
    fn wrap_spans_splits_by_column_budget_keeping_styles() {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let spans = vec![
            Span::styled("ab", bold),
            Span::styled("cd", Style::default()),
            Span::styled("ef", bold),
        ];
        let rows = wrap_spans(spans, 3);
        let plain: Vec<String> = rows
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(plain, vec!["abc", "def"]);
        // Span granularity is preserved: row0 = "ab"(bold) + "c"(plain),
        // row1 = "d"(plain) + "ef"(bold). Bold runs survive as one span.
        assert!(
            rows[0].spans[0].content == "ab"
                && rows[0].spans[0].style.add_modifier.contains(Modifier::BOLD),
            "row0 span0 = ab bold: {:?}",
            rows[0]
        );
        assert!(
            rows[0].spans[1].content == "c"
                && !rows[0].spans[1].style.add_modifier.contains(Modifier::BOLD),
            "row0 span1 = c plain: {:?}",
            rows[0]
        );
        assert!(
            rows[1].spans[0].content == "d"
                && !rows[1].spans[0].style.add_modifier.contains(Modifier::BOLD),
            "row1 span0 = d plain: {:?}",
            rows[1]
        );
        assert!(
            rows[1].spans[1].content == "ef"
                && rows[1].spans[1].style.add_modifier.contains(Modifier::BOLD),
            "row1 span1 = ef bold: {:?}",
            rows[1]
        );
    }

    #[test]
    fn wrap_spans_preserves_hard_newlines() {
        let spans = vec![Span::styled("ab\n\ncd", Style::default())];
        let rows = wrap_spans(spans, 4);
        let plain: Vec<String> = rows
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(plain, vec!["ab", "", "cd"]);
    }

    #[test]
    fn prefixed_wrap_keeps_prefix_on_first_row_and_pads() {
        let rows = prefixed_wrap(
            vec![Span::styled("> ", Style::default())],
            "abcdefghij",
            Style::default(),
            Style::default(),
            6,
        );
        // 2-col prefix + 4-col budget → "abcd"/"efgh"/"ij".
        assert_eq!(rows.len(), 3, "{rows:?}");
        let first: String = rows[0].spans.iter().map(|s| s.content.as_ref()).collect();
        let second: String = rows[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first.starts_with("> ab"), "row0: {first}");
        assert!(second.starts_with("  "), "row1 indented: {second}");
        // Every row padded to full width (prefix 2 + content, target 6).
        for row in &rows {
            let w: usize = row
                .spans
                .iter()
                .map(|s| paint_cols(s.content.as_ref()))
                .sum();
            assert_eq!(w, 6, "row padded: {row:?}");
        }
    }

    #[test]
    fn wrap_spans_keeps_cjk_glyphs_whole() {
        // CJK is 2 columns each; a 4-column budget holds exactly two glyphs.
        let rows = wrap_spans(vec![Span::styled("你好世界", Style::default())], 4);
        let plain: Vec<String> = rows
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(plain, vec!["你好", "世界"], "{plain:?}");
        // No row may split a CJK glyph into a half-width fragment.
        for row in &rows {
            let w: usize = row
                .spans
                .iter()
                .map(|s| paint_cols(s.content.as_ref()))
                .sum();
            assert!(w.is_multiple_of(2), "row width even for han text: {w}");
        }
    }

    #[test]
    fn wrap_spans_mixed_ascii_cjk_keeps_glyphs_whole() {
        // "abc你好" = 3 + 4 = 7 cols. A 4-col budget must not split "你".
        let rows = wrap_spans(vec![Span::styled("abc你好你好", Style::default())], 4);
        let plain: Vec<String> = rows
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(plain, vec!["abc", "你好", "你好"], "{plain:?}");
    }

    #[test]
    fn wrap_spans_wide_glyph_in_one_col_budget() {
        // A single 2-col CJK glyph cannot split; it is emitted on its own row
        // rather than dropped or halved.
        let rows = wrap_spans(vec![Span::styled("你a", Style::default())], 1);
        let plain: Vec<String> = rows
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(plain, vec!["你", "a"], "{plain:?}");
    }

    #[test]
    fn wrap_spans_does_not_split_emoji_zwj_sequence() {
        // 👨‍👩‍👧‍👦 is one grapheme of 2 columns despite 7 codepoints.
        let rows = wrap_spans(vec![Span::styled("👨‍👩‍👧‍👦ab", Style::default())], 2);
        let plain: Vec<String> = rows
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(plain, vec!["👨‍👩‍👧‍👦", "ab"], "{plain:?}");
    }

    #[test]
    fn wrap_spans_emoji_zwj_is_two_columns_not_eight() {
        // Two family emoji are 2+2 = 4 columns. A 3-column budget must place
        // each whole emoji on its own row, never counting them as 8 columns.
        let rows = wrap_spans(vec![Span::styled("👨‍👩‍👧‍👦👨‍👩‍👧‍👦", Style::default())], 3);
        let plain: Vec<String> = rows
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(plain, vec!["👨‍👩‍👧‍👦", "👨‍👩‍👧‍👦"], "{plain:?}");
    }

    #[test]
    fn wrap_spans_keeps_combining_mark_with_base() {
        let rows = wrap_spans(
            vec![Span::styled("e\u{0301}e\u{0301}", Style::default())],
            1,
        );
        let plain: Vec<String> = rows
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(plain, vec!["e\u{0301}", "e\u{0301}"], "{plain:?}");
    }
}
