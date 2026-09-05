use piko_tui_layout::{Component, SurfacePanel, ViewportState};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::Line,
    widgets::Paragraph,
};

use super::present::{detail_lines, empty_copy, row_line};
use super::{HistoryCtx, HistoryPanel};
use crate::app::HitId;
use crate::navigation::SurfaceId;
use crate::theme::Theme;
use crate::ui::components::pane::{PaneFooter, PaneSpec, PaneTitleAffix, render_pane};

const LENS_LABELS: [&str; 4] = ["Work", "Agents", "Transcript", "Journal"];

impl Component<HitId, HistoryCtx<'_>> for HistoryPanel {
    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &HistoryCtx<'_>,
        interaction: piko_tui_layout::InteractionState<HitId>,
    ) {
        self.render(frame, area, ctx);
        self.paint_hover(frame, area, ctx, interaction);
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &HistoryCtx<'_>) {
        self.last_width.set(area.width);
        let breadcrumb = self.breadcrumb();
        let spec = self.pane_spec(&breadcrumb, ctx.hints);
        let Some(areas) = render_pane(frame, area, &spec, ctx.theme) else {
            return;
        };
        let content = areas.content;
        if self.shows_detail_only() {
            self.render_lines(frame, content, self.detail_body(ctx.theme, content.width));
            return;
        }
        if self.is_wide() {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(46),
                    Constraint::Length(1),
                    Constraint::Min(24),
                ])
                .split(content);
            self.render_list(frame, columns[0], ctx.theme);
            self.render_gutter(frame, columns[1], ctx.theme);
            self.render_lines(
                frame,
                columns[2],
                self.detail_body(ctx.theme, columns[2].width),
            );
            return;
        }
        self.render_list(frame, content, ctx.theme);
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let breadcrumb = self.breadcrumb();
        let spec = self.pane_spec(&breadcrumb, None);
        let mut regions = spec
            .title_affix_regions(area)
            .into_iter()
            .filter_map(|(rect, hit)| match hit {
                crate::ui::components::pane::PaneAffixHit::ModeOption(i) => {
                    Some((rect, HitId::Mode(i)))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let Some(content) =
            crate::ui::components::pane::prepare_pane(area, &spec).map(|plan| plan.content)
        else {
            return regions;
        };
        if self.shows_detail_only() {
            regions.push((content, HitId::Content));
            return regions;
        }
        let list = if self.is_wide() {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(46),
                    Constraint::Length(1),
                    Constraint::Min(24),
                ])
                .split(content)[0]
        } else {
            content
        };
        let rows = self.visible_rows();
        let mut viewport = ViewportState::default();
        viewport.set_metrics(rows.len(), usize::from(list.height));
        viewport.ensure_visible(self.selected..self.selected.saturating_add(1));
        let visible = viewport.visible_range();
        for (offset, _) in rows
            .iter()
            .enumerate()
            .skip(visible.start)
            .take(visible.len())
        {
            let y = list.y.saturating_add((offset - visible.start) as u16);
            if y < list.y.saturating_add(list.height) {
                regions.push((Rect::new(list.x, y, list.width, 1), HitId::Row(offset)));
            }
        }
        if self.is_wide() {
            let detail = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(46),
                    Constraint::Length(1),
                    Constraint::Min(24),
                ])
                .split(content)[2];
            regions.push((detail, HitId::Content));
        }
        regions
    }
}

impl SurfacePanel<SurfaceId, HitId, HistoryCtx<'_>> for HistoryPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::History
    }
}

impl HistoryPanel {
    fn pane_spec<'a>(&'a self, breadcrumb: &'a str, hints: Option<&'a str>) -> PaneSpec<'a> {
        let rows = self.visible_rows();
        let at = if rows.is_empty() {
            0
        } else {
            self.selected.saturating_add(1)
        };
        let mut spec = PaneSpec::new("Session History")
            .title_affixes([
                PaneTitleAffix::mode_strip_static(&LENS_LABELS, self.lens.index()),
                PaneTitleAffix::selection(at, rows.len()),
            ])
            .tip(Some(breadcrumb))
            .focused(true);
        spec = if !self.filter.is_empty() || self.filter_editing {
            spec.search_filter(&self.filter)
        } else {
            spec.no_search()
        };
        if let Some(hints) = hints {
            spec.hints(hints)
        } else {
            spec.footer(PaneFooter::Reserved { height: 1 })
        }
    }

    fn render_gutter(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let lines = (0..area.height)
            .map(|_| Line::styled("│", Style::default().fg(theme.border_muted)))
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_list(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if self.loading {
            self.render_lines(
                frame,
                area,
                vec![Line::styled(
                    "Loading history…",
                    Style::default().fg(theme.muted),
                )],
            );
            return;
        }
        if let Some(error) = &self.error {
            self.render_lines(
                frame,
                area,
                vec![Line::styled(
                    error.clone(),
                    Style::default().fg(theme.error),
                )],
            );
            return;
        }
        let rows = self.visible_rows();
        if rows.is_empty() {
            let copy = if self.choosing_session {
                "No sessions match this filter."
            } else {
                empty_copy(self.lens)
            };
            self.render_lines(
                frame,
                area,
                vec![Line::styled(copy, Style::default().fg(theme.muted))],
            );
            return;
        }
        let lines = rows
            .iter()
            .enumerate()
            .map(|(index, row)| row_line(area.width, index == self.selected, row, theme))
            .collect();
        self.render_selected_lines(frame, area, lines, self.selected);
    }

    fn render_lines(&self, frame: &mut Frame<'_>, area: Rect, lines: Vec<Line<'static>>) {
        self.render_selected_lines(frame, area, lines, 0);
    }

    fn render_selected_lines(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        lines: Vec<Line<'static>>,
        selected: usize,
    ) {
        let mut viewport = self.viewport.get();
        viewport.set_metrics(lines.len(), usize::from(area.height));
        viewport.ensure_visible(selected..selected.saturating_add(1));
        let visible = viewport.visible_range();
        self.viewport.set(viewport);
        frame.render_widget(
            Paragraph::new(
                lines
                    .into_iter()
                    .skip(visible.start)
                    .take(visible.len())
                    .collect::<Vec<_>>(),
            ),
            area,
        );
    }

    fn detail_body(&self, theme: &Theme, width: u16) -> Vec<Line<'static>> {
        match &self.detail {
            Some(detail) => detail_lines(detail, theme, width),
            None => vec![Line::styled(
                "Select a row and press Enter to open it.",
                Style::default().fg(theme.muted),
            )],
        }
    }
}
