use piko_tui_layout::{Component, SplitAxis, SurfacePanel};
use ratatui::{Frame, layout::Rect, style::Style, widgets::Paragraph};

use super::layout::{HistoryLayout, LANE_GUTTER, short_agent_label};
use super::present::{empty_copy, row_line};
use super::{HistoryCtx, HistoryPanel};
use crate::ui::components::pane::{PaneFooter, PaneSpec, PaneTitleAffix, paint_pane};
use crate::{app::HitId, navigation::SurfaceId, theme::Theme};

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
        self.painted_lane_blocks.borrow_mut().clear();
        let breadcrumb = self.breadcrumb();
        let spec = self.pane_spec(&breadcrumb);
        let Some(layout) = self.prepare_layout(area) else {
            return;
        };
        let pane = &layout.pane;
        let split = layout.split;
        paint_pane(frame, pane, &spec, ctx.theme);
        if let (Some(footer), Some(hints)) = (pane.footer, ctx.hints) {
            frame.render_widget(
                Paragraph::new(hints).style(Style::default().fg(ctx.theme.muted)),
                footer,
            );
        }
        self.wide.set(split.is_wide());
        self.painted_split
            .set((!self.choosing_session && !self.agent_choosing).then_some(split));
        if !self.choosing_session && !self.agent_choosing {
            split.paint(frame, ctx.theme, self.active_pane);
            self.paint_lanes(frame, &layout, ctx.theme);
            if let Some(divider) = layout.lane_divider {
                crate::ui::components::divider::paint_divider_with_axis(
                    frame,
                    divider,
                    SplitAxis::Vertical,
                    ctx.theme,
                );
            }
        }
        if let (Some(list_area), Some(list_body)) = (layout.list_area, layout.list_body) {
            self.viewport.set(layout.list_viewport);
            self.render_list(frame, list_area, list_body, ctx.theme);
        }
        if !self.choosing_session
            && !self.agent_choosing
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
        let title = if self.agent_choosing {
            "Agent Streams"
        } else {
            "Session Trajectory"
        };
        // Revision and selection counts live in the breadcrumb; the title row
        // keeps only the agent switcher.
        let affixes = self
            .overview
            .as_ref()
            .map(|overview| {
                let active = overview
                    .agents
                    .iter()
                    .position(|agent| Some(&agent.agent_instance_id) == self.agent_id.as_ref())
                    .unwrap_or(0);
                vec![PaneTitleAffix::mode_strip(
                    overview.agents.iter().map(short_agent_label),
                    active,
                )]
            })
            .unwrap_or_default();
        let spec = PaneSpec::new(title)
            .title_affixes(affixes)
            .tip(Some(breadcrumb))
            .focused(true);
        let spec = if !self.filter.is_empty() || self.filter_editing {
            spec.search(crate::ui::components::pane::PaneSearch::Shown {
                filter: &self.filter,
                placeholder: Some("filter loaded rows"),
            })
        } else {
            spec.no_search()
        };
        spec.footer(PaneFooter::Reserved { height: 1 })
    }

    /// Paint the two lane rows and record their hit regions.
    fn paint_lanes(&self, frame: &mut Frame<'_>, layout: &HistoryLayout, theme: &Theme) {
        let Some((strip, rows)) = &layout.lane_rows else {
            return;
        };
        let Some(lanes) = &self.lanes else {
            return;
        };
        let selected_token = self
            .stream_item_at(self.selected)
            .map(|item| item.item_ref.token.clone());
        let labels = ["Model", "Tools"];
        let model_blocks: Vec<usize> = lanes
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.kind == piko_protocol::HistoryLaneBlockKind::ModelStep)
            .map(|(index, _)| index)
            .collect();
        let tool_blocks: Vec<usize> = lanes
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.kind == piko_protocol::HistoryLaneBlockKind::ToolCall)
            .map(|(index, _)| index)
            .collect();
        let mut lane_hits = Vec::new();
        let lane_height = strip.height / labels.len() as u16;
        for (lane_index, (label, block_ids)) in
            labels.iter().zip([model_blocks, tool_blocks]).enumerate()
        {
            let row = strip.y + lane_index as u16 * lane_height;
            frame.render_widget(
                Paragraph::new(crate::ui::line_layout::truncate_cols(
                    label,
                    usize::from(LANE_GUTTER.saturating_sub(1)),
                ))
                .style(Style::default().fg(theme.muted)),
                Rect::new(layout.pane.content.x, row, LANE_GUTTER.saturating_sub(1), 1),
            );
            let Some(rects) = rows.get(lane_index) else {
                continue;
            };
            // Lane colors mirror the stream badges: Model steps assistant,
            // tools tool-colored, so the two rows read as distinct lanes.
            let lane_color = if lane_index == 0 {
                theme.accent_assistant
            } else {
                theme.accent_tool
            };
            for (rect, &block_index) in rects.iter().zip(block_ids.iter()) {
                if rect.width == 0 {
                    continue;
                }
                let block = &lanes.blocks[block_index];
                let selected = selected_token.as_deref() == Some(block.reference.token.as_str());
                let failed = block.status.contains("failed") || block.status.contains("cancelled");
                let (fg, bg) = if selected {
                    (theme.bg_selected, lane_color)
                } else if failed {
                    (theme.error, theme.error)
                } else {
                    (lane_color, lane_color)
                };
                let style = Style::default().fg(fg).bg(bg);
                frame.buffer_mut().set_style(*rect, style);
                lane_hits.push((*rect, block_index));
            }
        }
        *self.painted_lane_blocks.borrow_mut() = lane_hits;
    }

    fn render_list(&self, frame: &mut Frame<'_>, area: Rect, list: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let row_count = self.row_count();
        let status = self.list_status();
        let header_height = u16::from(!status.is_empty()).min(area.height);
        if header_height > 0 {
            frame.render_widget(
                Paragraph::new(status).style(Style::default().fg(theme.muted)),
                Rect::new(area.x, area.y, area.width, 1),
            );
        }
        let body = Rect::new(
            area.x,
            area.y + header_height,
            area.width,
            area.height.saturating_sub(header_height),
        );
        if row_count == 0 {
            let copy = if self.loading {
                "Loading trajectory…"
            } else if let Some(error) = &self.error {
                error
            } else if !self.filter.is_empty() {
                "No loaded rows match this filter."
            } else if self.choosing_session {
                "No sessions found."
            } else {
                empty_copy(self.agent_choosing)
            };
            frame.render_widget(
                Paragraph::new(copy).style(Style::default().fg(theme.muted)),
                body,
            );
            return;
        }
        let visible = self.viewport.get().visible_range();
        let first_visible = visible.start;
        let rows = self.visible_rows_range(visible);
        let lines = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                row_line(
                    list.width.saturating_sub(2),
                    first_visible + index == self.selected,
                    row,
                    theme,
                )
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
