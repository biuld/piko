//! Compact table layout for Markdown documents.

use gpui::*;

use crate::theme::{ChromeTokens, TextRole, metrics, text, tokens};

use super::super::model::{MarkdownAlignment, MarkdownTable, MarkdownTableCell};
use super::inline::styled_inline;

pub(crate) fn render_table(table: &MarkdownTable) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let mut root = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .rounded_md()
        .border_1()
        .border_color(ChromeTokens::hsla(t.border));

    if !table.head.is_empty() {
        root = root.child(render_row(&table.head, &table.alignments, true, m.space_sm));
    }
    for row in &table.rows {
        root = root.child(render_row(row, &table.alignments, false, m.space_sm));
    }
    root.into_any_element()
}

fn render_row(
    cells: &[MarkdownTableCell],
    alignments: &[MarkdownAlignment],
    head: bool,
    padding: Pixels,
) -> AnyElement {
    let t = tokens();
    let mut row = div().w_full().min_w_0().flex().flex_row();
    if head {
        row = row
            .bg(t.elevated_rgba())
            .border_b_1()
            .border_color(ChromeTokens::hsla(t.border));
    }
    for (index, cell) in cells.iter().enumerate() {
        let alignment = alignments
            .get(index)
            .copied()
            .unwrap_or(MarkdownAlignment::None);
        let mut cell_el = text(TextRole::Body)
            .flex_1()
            .min_w_0()
            .p(padding)
            .text_color(t.fg_rgba())
            .child(styled_inline(&cell.content, head));
        cell_el = match alignment {
            MarkdownAlignment::Center => cell_el.text_center(),
            MarkdownAlignment::Right => cell_el.text_right(),
            MarkdownAlignment::None | MarkdownAlignment::Left => cell_el.text_left(),
        };
        if index > 0 {
            cell_el = cell_el
                .border_l_1()
                .border_color(ChromeTokens::hsla(t.border));
        }
        row = row.child(cell_el);
    }
    row.into_any_element()
}
