//! Scrollable detail and local request feedback for the opened item.
use super::{
    HistoryPanel,
    present::{detail_lines, row_context, row_line},
};
use crate::{theme::Theme, ui::components::split_pane::PaneSide};
use ratatui::{Frame, layout::Rect, style::Style, text::Line, widgets::Paragraph};

impl HistoryPanel {
    pub(super) fn render_detail(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let focused = self.active_pane == PaneSide::Second;
        let body = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        );
        let lines = if self.detail_loading || self.detail_error.is_some() {
            let mut lines = Vec::new();
            if let Some(row) = &self.opened_row {
                lines.push(row_line(body.width, false, row, theme));
            }
            let (message, color) = if let Some(error) = &self.detail_error {
                (format!("{error} · open again to retry"), theme.error)
            } else {
                ("Loading selected detail…".into(), theme.muted)
            };
            lines.extend(super::present::feedback_lines(&message, color, body.width));
            if let Some(row) = &self.opened_row {
                lines.push(Line::default());
                lines.extend(row_context(row, theme, body.width));
            }
            lines
        } else {
            self.detail_body(theme, body.width)
        };
        let mut viewport = self.detail_viewport.get();
        viewport.set_metrics(lines.len(), usize::from(body.height));
        let visible = viewport.visible_range();
        self.detail_viewport.set(viewport);
        let title = format!(
            "Detail · {}–{} / {}",
            if lines.is_empty() {
                0
            } else {
                visible.start + 1
            },
            visible.end,
            lines.len()
        );
        frame.render_widget(
            Paragraph::new(title).style(Style::default().fg(if focused {
                theme.accent
            } else {
                theme.muted
            })),
            Rect::new(area.x, area.y, area.width, 1),
        );
        frame.render_widget(
            Paragraph::new(
                lines
                    .into_iter()
                    .skip(visible.start)
                    .take(visible.len())
                    .collect::<Vec<_>>(),
            ),
            body,
        );
    }

    fn detail_body(&self, theme: &Theme, width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(row) = &self.opened_row {
            lines.push(row_line(width, false, row, theme));
        }
        match &self.detail {
            Some(detail) => lines.extend(detail_lines(detail, theme, width)),
            None => {
                let summary_row = self
                    .opened_row
                    .as_ref()
                    .cloned()
                    .or_else(|| self.visible_rows().get(self.selected).cloned());
                if let Some(row) = summary_row.as_ref() {
                    lines.extend(row_context(row, theme, width));
                    if let super::HistoryRow::CommitHeader { revision, .. } = row
                        && let Some(commit) = self.journal.as_ref().and_then(|page| {
                            page.commits
                                .iter()
                                .find(|commit| commit.revision == *revision)
                        })
                    {
                        for (label, value) in [
                            ("Commit ID", Some(&commit.commit_id)),
                            ("Causation", commit.causation_id.as_ref()),
                            ("Correlation", commit.correlation_id.as_ref()),
                        ] {
                            if let Some(value) = value {
                                lines.extend(
                                    crate::ui::line_layout::soft_wrap(
                                        &format!("{label}\n{value}"),
                                        usize::from(width.max(1)),
                                    )
                                    .into_iter()
                                    .map(Line::from),
                                );
                            }
                        }
                    }
                }
                let hint = match summary_row.as_ref() {
                    Some(super::HistoryRow::Item { item, .. }) if item.has_detail => {
                        "Loaded summary. Open the item to inspect its recorded detail."
                    }
                    Some(super::HistoryRow::Transcript(item)) if item.has_detail => {
                        "Loaded summary. Open the item to inspect its recorded detail."
                    }
                    _ => "Loaded summary. Back returns to the list.",
                };
                lines.extend(super::present::feedback_lines(hint, theme.muted, width));
            }
        }
        if self.detail.is_some()
            && let Some(row) = &self.opened_row
        {
            lines.extend(row_context(row, theme, width));
        }
        lines
    }
}
