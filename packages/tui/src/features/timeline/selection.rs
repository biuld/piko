//! Mouse-driven text selection for the rendered Timeline transcript.

use std::ops::Range;

use ratatui::{Frame, layout::Rect, style::Style, text::Line};

use crate::terminal::text::TerminalTextPolicy;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct SelectionPoint {
    pub row: usize,
    pub col: u16,
}

#[derive(Default)]
pub(crate) struct TimelineSelection {
    anchor: Option<SelectionPoint>,
    head: Option<SelectionPoint>,
    dragging: bool,
    moved: bool,
    lines: Vec<String>,
    epoch: u64,
}

impl TimelineSelection {
    pub(crate) fn update_snapshot(&mut self, lines: &[Line<'_>], epoch: u64) {
        if self.epoch != epoch && self.anchor.is_some() {
            self.clear();
        }
        self.epoch = epoch;
        self.lines = lines.iter().map(Line::to_string).collect();
    }

    pub(crate) fn start(&mut self, point: SelectionPoint) {
        self.anchor = Some(point);
        self.head = Some(point);
        self.dragging = true;
        self.moved = false;
    }

    pub(crate) fn update(&mut self, point: SelectionPoint) {
        if self.dragging {
            self.moved |= self.anchor != Some(point);
            self.head = Some(point);
        }
    }

    pub(crate) fn finish(&mut self, point: SelectionPoint) -> bool {
        self.update(point);
        self.dragging = false;
        if !self.moved {
            self.clear();
            false
        } else {
            true
        }
    }

    pub(crate) fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
        self.moved = false;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.bounds().is_some()
    }

    pub(crate) fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub(crate) fn selected_text(&self) -> Option<String> {
        let (start, end) = self.bounds()?;
        let mut selected = Vec::new();
        for row in start.row..=end.row {
            let line = self.lines.get(row)?;
            let from = if row == start.row { start.col } else { 0 };
            let to = if row == end.row { end.col } else { u16::MAX };
            selected.push(slice_columns(line, from..to).trim_end().to_string());
        }
        let text = selected.join("\n");
        (!text.is_empty()).then_some(text)
    }

    pub(crate) fn paint(
        &self,
        frame: &mut Frame<'_>,
        content: Rect,
        top: usize,
        visible_rows: usize,
        style: Style,
    ) {
        let Some((start, end)) = self.bounds() else {
            return;
        };
        let visible_end = top.saturating_add(visible_rows);
        for row in start.row.max(top)..=end.row.min(visible_end.saturating_sub(1)) {
            let from = if row == start.row { start.col } else { 0 };
            let to = if row == end.row {
                end.col
            } else {
                content.width
            };
            let from = from.min(content.width);
            let to = to.min(content.width);
            if from < to {
                frame.buffer_mut().set_style(
                    Rect::new(
                        content.x.saturating_add(from),
                        content.y.saturating_add((row - top) as u16),
                        to - from,
                        1,
                    ),
                    style,
                );
            }
        }
    }

    fn bounds(&self) -> Option<(SelectionPoint, SelectionPoint)> {
        let anchor = self.anchor?;
        let head = self.head?;
        if anchor == head || !self.moved {
            return None;
        }
        if anchor < head {
            Some((anchor, after_cell(head)))
        } else {
            Some((head, after_cell(anchor)))
        }
    }
}

fn after_cell(point: SelectionPoint) -> SelectionPoint {
    SelectionPoint {
        row: point.row,
        col: point.col.saturating_add(1),
    }
}

fn slice_columns(text: &str, columns: Range<u16>) -> String {
    let policy = TerminalTextPolicy;
    let mut result = String::new();
    let mut col = 0usize;
    for grapheme in policy.graphemes(text) {
        let width = policy.width(grapheme);
        let next = col.saturating_add(width);
        if next > usize::from(columns.start) && col < usize::from(columns.end) {
            result.push_str(grapheme);
        }
        col = next;
        if col >= usize::from(columns.end) {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_copies_wrapped_rows_and_wide_glyphs() {
        let mut selection = TimelineSelection::default();
        selection.update_snapshot(&[Line::from("ab界d"), Line::from("next  ")], 1);
        selection.start(SelectionPoint { row: 0, col: 1 });
        selection.update(SelectionPoint { row: 1, col: 2 });
        assert!(selection.finish(SelectionPoint { row: 1, col: 2 }));
        assert_eq!(selection.selected_text().as_deref(), Some("b界d\nnex"));
    }

    #[test]
    fn click_clears_selection_without_becoming_active() {
        let mut selection = TimelineSelection::default();
        let point = SelectionPoint { row: 2, col: 3 };
        selection.start(point);
        assert!(!selection.finish(point));
        assert!(!selection.is_active());
    }
}
