//! Scrollable tabbed detail for the opened item.
//!
//! Pane layout (top to bottom): the tab strip, then the active tab's content.
//! Summary owns row and journal metadata; other tabs show only their payload.
use super::{HistoryPanel, HistoryRow, present::tab_lines};
use crate::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::Paragraph,
};

pub(super) const TAB_LABELS: [&str; 4] = ["Summary", "Payload", "Result", "Timing"];

#[derive(Clone, PartialEq, Eq)]
struct DetailCacheKey {
    detail: Option<(u64, String)>,
    opened: Option<String>,
    tab: super::DetailTab,
    width: u16,
    colors: [Color; 9],
}

pub(super) struct CachedDetail {
    key: DetailCacheKey,
    lines: Vec<Line<'static>>,
}

impl HistoryPanel {
    pub(super) fn render_detail(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let tabs = self.tab_rects(area);
        // One row for the tab strip plus one blank row between it and content.
        let tabs_height = u16::from(!tabs.is_empty()) * 2;
        let body = Rect::new(
            area.x,
            area.y + tabs_height,
            area.width,
            area.height.saturating_sub(tabs_height),
        );
        if self.detail_loading || self.detail_error.is_some() {
            let (message, color) = if let Some(error) = &self.detail_error {
                (format!("{error} · reopen to retry"), theme.error)
            } else {
                ("Loading selected detail…".into(), theme.muted)
            };
            let lines = super::present::feedback_lines(&message, color, body.width);
            self.paint_detail_lines(frame, body, &lines);
            if !tabs.is_empty() {
                self.paint_tabs(frame, &tabs, theme);
            }
            return;
        }
        let key = self.detail_cache_key(theme, body.width);
        let stale = self
            .detail_render_cache
            .borrow()
            .as_ref()
            .is_none_or(|cached| cached.key != key);
        if stale {
            let lines = self.detail_body(theme, body.width);
            *self.detail_render_cache.borrow_mut() = Some(CachedDetail { key, lines });
        }
        let cached = self.detail_render_cache.borrow();
        let lines = &cached.as_ref().expect("detail cache populated").lines;
        self.paint_detail_lines(frame, body, lines);
        if !tabs.is_empty() {
            self.paint_tabs(frame, &tabs, theme);
        }
    }

    fn paint_detail_lines(&self, frame: &mut Frame<'_>, body: Rect, lines: &[Line<'static>]) {
        let mut viewport = self.detail_viewport.get();
        viewport.set_metrics(lines.len(), usize::from(body.height));
        let visible = viewport.visible_range();
        self.detail_viewport.set(viewport);
        frame.render_widget(Paragraph::new(lines[visible].to_vec()), body);
    }

    fn detail_cache_key(&self, theme: &Theme, width: u16) -> DetailCacheKey {
        let opened = match self.opened_row.as_ref() {
            Some(HistoryRow::Stream(item)) => Some(item.item_ref.token.clone()),
            _ => None,
        };
        DetailCacheKey {
            detail: self
                .detail
                .as_ref()
                .map(|detail| (detail.item_ref.revision, detail.item_ref.token.clone())),
            opened,
            tab: self.detail_tab,
            width,
            colors: [
                theme.text,
                theme.dim,
                theme.muted,
                theme.warning,
                theme.accent,
                theme.accent_user,
                theme.accent_assistant,
                theme.thinking_text,
                theme.error,
            ],
        }
    }

    fn paint_tabs(&self, frame: &mut Frame<'_>, tabs: &[Rect], theme: &Theme) {
        for (index, rect) in tabs.iter().enumerate() {
            let active = index == self.detail_tab.index();
            let inner = crate::ui::line_layout::truncate_cols(
                TAB_LABELS[index],
                usize::from(rect.width.saturating_sub(2)),
            );
            let label = if active {
                format!("[{inner}]")
            } else {
                format!(" {inner} ")
            };
            frame.render_widget(
                Paragraph::new(crate::ui::line_layout::truncate_cols(
                    &label,
                    usize::from(rect.width),
                ))
                .style(Style::default().fg(if active {
                    theme.accent
                } else {
                    theme.muted
                })),
                *rect,
            );
        }
    }

    pub(super) fn tab_rects(&self, area: Rect) -> Vec<Rect> {
        tab_hit_rects(area)
            .into_iter()
            .map(|(rect, _)| rect)
            .collect()
    }

    fn detail_body(&self, theme: &Theme, width: u16) -> Vec<Line<'static>> {
        tab_lines(
            self.detail_tab,
            self.detail.as_ref(),
            self.opened_row.as_ref(),
            theme,
            width,
        )
    }
}

/// Shared tab geometry for paint and pointer hit regions.
pub(super) fn tab_hit_rects(area: Rect) -> Vec<(ratatui::layout::Rect, usize)> {
    if area.width < 20 || area.height == 0 {
        return Vec::new();
    }
    let per = (area.width / TAB_LABELS.len() as u16).min(12);
    let mut x = area.x;
    TAB_LABELS
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let width = (crate::ui::line_layout::paint_cols(label) as u16 + 2)
                .min(per)
                .min(area.right().saturating_sub(x));
            let rect = Rect::new(x, area.y, width, 1);
            x += per;
            (rect, index)
        })
        .collect()
}
