//! Multi-column (table) body paint for selectable rows.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Row, Table, TableState},
};

use super::{ColumnAlign, ColumnCell, ColumnCellStyle, SelectableItem};
use crate::theme::Theme;
use crate::ui::components::feedback::{
    active_marker_span, row_primary_style, selection_prefix, with_selected_bg,
};

/// Rows already filtered; `selected` is an index into this slice (filtered order).
pub(super) fn paint_column_items(
    frame: &mut Frame<'_>,
    content: Rect,
    items: &[&SelectableItem],
    selected: usize,
    theme: &Theme,
) {
    paint_column_items_with_widths(frame, content, items, selected, theme, None);
}

pub(super) fn paint_column_items_with_widths(
    frame: &mut Frame<'_>,
    content: Rect,
    items: &[&SelectableItem],
    selected: usize,
    theme: &Theme,
    widths: Option<&[Constraint]>,
) {
    let owned: Vec<Vec<ColumnCell>> = items.iter().map(|i| i.resolved_cells()).collect();
    let rows: Vec<ColumnPaintRow<'_>> = items
        .iter()
        .zip(owned.iter())
        .map(|(item, cells)| ColumnPaintRow {
            cells: cells.as_slice(),
            is_active: item.is_active,
        })
        .collect();
    paint_column_rows(frame, content, &rows, selected, theme, widths);
}

/// Public table paint used by feature surfaces that already own Pane chrome
/// (auto-completion Suggest dock).
///
/// `widths` are **data column** constraints (no selection caret). When `None`,
/// widths are derived from cell content (palette shape).
pub fn paint_column_rows(
    frame: &mut Frame<'_>,
    content: Rect,
    rows: &[ColumnPaintRow<'_>],
    selected: usize,
    theme: &Theme,
    widths: Option<&[Constraint]>,
) {
    if rows.is_empty() {
        return;
    }

    let num_cols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    if num_cols == 0 {
        return;
    }

    let mut constraints = Vec::with_capacity(num_cols + 1);
    constraints.push(Constraint::Length(2)); // selection caret
    if let Some(custom) = widths {
        // Use provided constraints; pad/truncate to num_cols.
        for i in 0..num_cols {
            constraints.push(custom.get(i).copied().unwrap_or(Constraint::Min(1)));
        }
    } else {
        let mut max_col_widths = vec![0usize; num_cols];
        for row in rows {
            for (col_idx, cell) in row.cells.iter().enumerate() {
                max_col_widths[col_idx] = max_col_widths[col_idx].max(cell.text.chars().count());
            }
            if row.is_active
                && let Some(last) = max_col_widths.last_mut()
            {
                *last = (*last).saturating_add(2);
            }
        }
        for width in max_col_widths.iter_mut().take(num_cols.saturating_sub(1)) {
            *width = (*width).min(40);
        }
        for (col_idx, width) in max_col_widths.iter().enumerate() {
            let width = (*width as u16).max(1);
            if col_idx < num_cols - 1 {
                constraints.push(Constraint::Length(width.saturating_add(2)));
            } else {
                constraints.push(Constraint::Min(width));
            }
        }
    }

    let table_rows: Vec<Row<'_>> = rows
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let is_selected = idx == selected;
            let marker_style = if is_selected {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let mut cells = vec![Cell::from(Line::from(Span::styled(
                selection_prefix(is_selected),
                marker_style,
            )))];

            for (col_idx, cell) in row.cells.iter().enumerate() {
                let base = cell_style(cell.style, is_selected, theme);
                let mut spans = vec![Span::styled(cell.text.clone(), base)];
                if row.is_active && col_idx + 1 == row.cells.len() {
                    spans.push(active_marker_span(theme));
                }
                let mut line = Line::from(spans);
                if cell.align == ColumnAlign::Right {
                    line = line.alignment(Alignment::Right);
                }
                cells.push(Cell::from(line));
            }
            while cells.len() < num_cols + 1 {
                cells.push(Cell::from(""));
            }
            Row::new(cells)
        })
        .collect();

    let table = Table::new(table_rows, constraints).row_highlight_style(with_selected_bg(
        row_primary_style(true, theme),
        true,
        theme,
    ));

    let mut state = TableState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(table, content, &mut state);
}

fn cell_style(style: ColumnCellStyle, is_selected: bool, theme: &Theme) -> Style {
    match style {
        ColumnCellStyle::Primary => {
            with_selected_bg(row_primary_style(is_selected, theme), is_selected, theme)
        }
        ColumnCellStyle::Secondary => Style::default().fg(theme.dim),
        ColumnCellStyle::Emphasized => {
            let base = if is_selected {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
            };
            with_selected_bg(base, is_selected, theme)
        }
        ColumnCellStyle::Status => Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    }
}

/// Intermediate borrow-friendly row for column paint.
pub(super) struct ColumnPaintRow<'a> {
    pub cells: &'a [ColumnCell],
    pub is_active: bool,
}

/// Single-column rich lines (tree connectors, mixed colors). Uses [`List`].
pub fn paint_rich_lines(
    frame: &mut Frame<'_>,
    content: Rect,
    lines: &[Line<'static>],
    selected: usize,
    theme: &Theme,
) {
    use ratatui::widgets::{List, ListItem, ListState};

    if lines.is_empty() {
        return;
    }
    let items: Vec<ListItem<'_>> = lines.iter().cloned().map(ListItem::new).collect();
    let list = List::new(items).highlight_style(with_selected_bg(
        row_primary_style(true, theme),
        true,
        theme,
    ));
    let mut state = ListState::default();
    state.select(Some(selected.min(lines.len().saturating_sub(1))));
    frame.render_stateful_widget(list, content, &mut state);
}
