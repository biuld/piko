use piko_tui_layout::{Component, SurfacePanel};
use ratatui::{Frame, layout::Rect, style::Style, widgets::Paragraph};

use super::present::{empty_copy, row_line};
use super::{HistoryCtx, HistoryPanel};
use crate::ui::components::pane::{PaneFooter, PaneSpec, PaneTitleAffix, paint_pane};
use crate::{app::HitId, navigation::SurfaceId, theme::Theme};

pub(super) const LENS_LABELS: [&str; 4] = ["Work", "Agents", "Transcript", "Journal"];

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
        self.painted_regions.borrow_mut().clear();
        let breadcrumb = self.breadcrumb();
        let spec = self.pane_spec(&breadcrumb);
        let Some(layout) = self.prepare_layout(area) else {
            return;
        };
        let pane = &layout.pane;
        let split = layout.split;
        paint_pane(frame, pane, &spec, ctx.theme);
        for (index, rect) in layout.tabs.iter().enumerate() {
            let active = index == self.lens.index();
            let inner = crate::ui::line_layout::truncate_cols(
                LENS_LABELS[index],
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
                    ctx.theme.accent
                } else {
                    ctx.theme.muted
                })),
                *rect,
            );
        }
        if let (Some(footer), Some(hints)) = (pane.footer, ctx.hints) {
            frame.render_widget(
                Paragraph::new(hints).style(Style::default().fg(ctx.theme.muted)),
                footer,
            );
        }
        self.wide.set(split.is_wide());
        self.painted_split
            .set((!self.choosing_session).then_some(split));
        if !self.choosing_session {
            split.paint(frame, ctx.theme, self.active_pane);
        }
        if let (Some(list_area), Some(list_body)) = (layout.list_area, layout.list_body) {
            self.viewport.set(layout.list_viewport);
            self.render_list(frame, list_area, list_body, ctx.theme);
        }
        if !self.choosing_session
            && let Some(second) = split.second
        {
            self.render_detail(frame, second.content, ctx.theme);
        }
        *self.painted_regions.borrow_mut() = layout.hits;
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        self.prepare_layout(area)
            .map(|layout| layout.hits)
            .unwrap_or_default()
    }
}

impl SurfacePanel<SurfaceId, HitId, HistoryCtx<'_>> for HistoryPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::History
    }
}

impl HistoryPanel {
    pub(super) fn pane_spec<'a>(&'a self, breadcrumb: &'a str) -> PaneSpec<'a> {
        let rows = self.visible_rows();
        let at = if rows.is_empty() {
            0
        } else {
            self.selected.min(rows.len() - 1) + 1
        };
        let spec = PaneSpec::new("Session History")
            .title_affixes([
                PaneTitleAffix::label(
                    self.overview
                        .as_ref()
                        .map(|overview| format!("r{}", overview.revision))
                        .unwrap_or_else(|| "Sessions".into()),
                ),
                PaneTitleAffix::selection(at, rows.len()),
            ])
            .tip(Some(breadcrumb))
            .focused(true);
        let spec = if !self.filter.is_empty() || self.filter_editing {
            spec.search(crate::ui::components::pane::PaneSearch::Shown {
                filter: &self.filter,
                placeholder: Some("filter loaded summaries"),
            })
        } else {
            spec.no_search()
        };
        spec.footer(PaneFooter::Reserved { height: 1 })
    }

    fn render_list(&self, frame: &mut Frame<'_>, area: Rect, list: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let rows = self.visible_rows();
        let scope = match self.provenance {
            piko_protocol::HistoryProvenanceFilter::All => "facts + diag",
            piko_protocol::HistoryProvenanceFilter::Facts => "facts only",
            piko_protocol::HistoryProvenanceFilter::Diagnostics => "diag only",
        };
        let counts = if self.filter.is_empty() {
            format!("{} loaded", rows.len())
        } else {
            format!("{} / {} loaded", rows.len(), self.loaded_row_count())
        };
        let title = format!(
            "{counts}{}{} · {scope}",
            if self.loading { " · loading…" } else { "" },
            if self.has_more() { " · more" } else { "" },
        );
        frame.render_widget(
            Paragraph::new(title).style(Style::default().fg(theme.muted)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        let body = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        );
        if rows.is_empty() {
            let copy = if self.loading {
                "Loading history…"
            } else if let Some(error) = &self.error {
                error
            } else if !self.filter.is_empty() {
                "No loaded summaries match this filter."
            } else if self.choosing_session {
                "No sessions found."
            } else {
                empty_copy(self.lens)
            };
            frame.render_widget(
                Paragraph::new(copy).style(Style::default().fg(theme.muted)),
                body,
            );
            return;
        }
        let visible = self.viewport.get().visible_range();
        let lines = rows
            .iter()
            .enumerate()
            .skip(visible.start)
            .take(visible.len())
            .map(|(index, row)| {
                let mut line = row_line(
                    list.width.saturating_sub(2),
                    index == self.selected,
                    row,
                    theme,
                );
                if list.width >= 2 {
                    line.spans.push(ratatui::text::Span::styled(
                        " i",
                        Style::default().fg(theme.muted),
                    ));
                }
                line
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), list);
        if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(format!("{error} · refresh to retry"))
                    .style(Style::default().fg(theme.error)),
                Rect::new(
                    body.x,
                    list.bottom(),
                    body.width,
                    u16::from(body.height > 0),
                ),
            );
        }
    }
}
