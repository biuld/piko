//! One geometry recipe shared by frame preparation and painting.
use super::HistoryPanel;
use super::detail::tab_hit_rects;
use crate::{
    app::HitId,
    ui::components::{
        pane::{PaneAffixHit, PanePadding, PanePlan, prepare_pane},
        split_pane::{SplitPanePlan, SplitPaneSpec},
    },
};
use piko_tui_layout::{SplitSize, ViewportState};
use ratatui::layout::Rect;

/// Solved lane strip: (strip rect, per-lane block rects), hit rects, and
/// whether the strip runs in sequence mode.
/// Columns reserved left of the strip for the per-lane labels.
pub(super) const LANE_GUTTER: u16 = 7;

/// Native columns per sequence unit when the strip scrolls horizontally.
pub(super) const LANE_UNIT: u16 = 2;

/// Blank columns between step clusters in scroll mode.
const LANE_CLUSTER_GAP: u64 = 2;

/// Native strip positions (in columns) per block, clustering ticks that close
/// with a ModelStep tick: a step's boundary is the last block of its cluster
/// in journal order, so ticks touch inside a cluster and a gap separates the
/// next cluster.
pub(super) fn lane_cluster_positions(lanes: &piko_protocol::HistoryLaneSummary) -> Vec<u64> {
    let mut positions = Vec::with_capacity(lanes.blocks.len());
    let mut axis = 0u64;
    let mut gap_pending = false;
    for block in &lanes.blocks {
        if gap_pending {
            axis += LANE_CLUSTER_GAP;
        }
        positions.push(axis);
        axis += 1;
        gap_pending = block.kind == piko_protocol::HistoryLaneBlockKind::ModelStep;
    }
    positions
}

pub(super) type LaneSolve = (Option<(Rect, Vec<Vec<Rect>>)>, Vec<(Rect, usize)>);

pub(super) struct HistoryLayout {
    pub pane: PanePlan,
    pub split: SplitPanePlan,
    pub list_area: Option<Rect>,
    pub list_body: Option<Rect>,
    pub list_viewport: ViewportState,
    pub hits: Vec<(Rect, HitId)>,
    /// One row per lane (Model, Tools); each row holds solved block rects in
    /// that lane's block order.
    pub lane_rows: Option<(Rect, Vec<Vec<Rect>>)>,
    pub lane_divider: Option<Rect>,
}

impl HistoryPanel {
    pub(super) fn prepare_layout(&self, area: Rect) -> Option<HistoryLayout> {
        let breadcrumb = self.breadcrumb();
        let spec = self.pane_spec(&breadcrumb);
        let pane = prepare_pane(area, &spec)?;
        if self.choosing_session || self.agent_choosing || pane.content.height == 0 {
            return Some(self.list_only_layout(&pane));
        }
        let agents = pane
            .affix_hits
            .iter()
            .filter_map(|(rect, hit)| match hit {
                PaneAffixHit::ModeOption(index) => Some((*rect, *index)),
                PaneAffixHit::Close => None,
            })
            .map(|(rect, index)| (rect, HitId::Mode(index)))
            .collect::<Vec<_>>();
        let content = pane.content;
        let (lane_rows, lane_hits) = self.solve_lane_strip(content);
        let lane_count = lane_rows
            .as_ref()
            .map(|(_, rows)| rows.len() as u16)
            .unwrap_or(0);
        let lane_divider = (lane_count > 0 && content.height > lane_count)
            .then(|| Rect::new(content.x, content.y + lane_count, content.width, 1));
        let reserved = lane_count
            .saturating_add(u16::from(lane_divider.is_some()))
            .min(content.height);
        let content = Rect::new(
            content.x,
            content.y + reserved,
            content.width,
            content.height.saturating_sub(reserved),
        );
        let split = SplitPaneSpec {
            first: SplitSize::Percent(46),
            minimum: [34, 42],
            padding: PanePadding::new(1, 0),
            separator: 1,
        }
        .prepare(content, self.active_pane);
        let list_area = split.first.map(|region| region.content);
        let list_body = list_area.map(|area| {
            let header_height = self.list_header_height().min(area.height);
            Rect::new(
                area.x,
                area.y.saturating_add(header_height),
                area.width,
                area.height
                    .saturating_sub(header_height)
                    .saturating_sub(u16::from(self.error.is_some())),
            )
        });
        let mut viewport = self.viewport.get();
        if let Some(list) = list_body {
            viewport.set_metrics(self.row_count(), usize::from(list.height));
            viewport.ensure_visible(self.selected..self.selected.saturating_add(1));
        }
        let mut hits = agents;
        hits.extend(
            lane_hits
                .iter()
                .map(|(rect, index)| (*rect, HitId::Lane(*index))),
        );
        if let Some(detail) = split.second {
            hits.push((detail.content, HitId::Content));
            for (rect, index) in tab_hit_rects(detail.content) {
                hits.push((rect, HitId::Tab(index)));
            }
        }
        if let Some(list) = list_body {
            hits.extend(row_hit_rects(list, &viewport));
        }
        Some(HistoryLayout {
            pane,
            split,
            list_area,
            list_body,
            list_viewport: viewport,
            hits,
            lane_rows,
            lane_divider,
        })
    }

    fn list_only_layout(&self, pane: &PanePlan) -> HistoryLayout {
        self.lane_strip_rect.set(None);
        self.lane_unit.set(0);
        let header_height = self.list_header_height().min(pane.content.height);
        let list_body = Rect::new(
            pane.content.x,
            pane.content.y.saturating_add(header_height),
            pane.content.width,
            pane.content
                .height
                .saturating_sub(header_height)
                .saturating_sub(u16::from(self.error.is_some())),
        );
        let mut viewport = self.viewport.get();
        viewport.set_metrics(self.row_count(), usize::from(list_body.height));
        viewport.ensure_visible(self.selected..self.selected.saturating_add(1));
        let hits = row_hit_rects(list_body, &viewport);
        HistoryLayout {
            pane: pane.clone(),
            split: SplitPanePlan {
                first: None,
                second: None,
                divider: None,
            },
            list_area: Some(pane.content),
            list_body: Some(list_body),
            list_viewport: viewport,
            hits,
            lane_rows: None,
            lane_divider: None,
        }
    }

    pub(super) fn list_status(&self) -> String {
        let counts = if self.filter.is_empty() {
            String::new()
        } else {
            format!("{} / {} loaded", self.row_count(), self.loaded_row_count())
        };
        format!("{counts}{}", if self.loading { " loading…" } else { "" })
    }

    pub(super) fn list_header_height(&self) -> u16 {
        u16::from(!self.list_status().is_empty())
    }

    /// Solve the two-lane strip (Model steps, Tool calls) and produce hit
    /// regions that map block rects back to `lanes.blocks` indexes.
    ///
    /// When every block fits (sequence span within the strip width), blocks
    /// spread across the full width. When the span overflows, the strip keeps
    /// a native `LANE_UNIT`-column scale and scrolls horizontally through the
    /// shared `lane_viewport` instead of compressing ticks onto one column.
    fn solve_lane_strip(&self, content: Rect) -> LaneSolve {
        self.lane_strip_rect.set(None);
        self.lane_unit.set(0);
        let Some(lanes) = &self.lanes else {
            return (None, Vec::new());
        };
        if content.height < 2 || content.width < LANE_GUTTER + 8 {
            return (None, Vec::new());
        }
        let strip = Rect::new(
            content.x + LANE_GUTTER,
            content.y,
            content.width - LANE_GUTTER,
            2,
        );
        self.lane_strip_rect.set(Some(strip));
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
        // Thin ticks on one shared journal-order axis. Blocks stay narrow so
        // the two lanes read as separate tick rows instead of a merged band.
        // Scroll mode groups consecutive blocks of one model step into a
        // cluster: ticks touch inside a cluster and a gap separates clusters,
        // so a step and its tool calls read as one unit. Fit mode keeps the
        // stream-proportional spread across the full width.
        let max_sequence = lanes
            .blocks
            .iter()
            .map(|block| block.sequence)
            .max()
            .unwrap_or(0);
        let block_count = u64::try_from(lanes.blocks.len()).unwrap_or(u64::MAX);
        let usable = u64::from(strip.width.saturating_sub(1));
        let viewport_offset = self.lane_viewport.get().top();
        let scroll_mode = block_count * u64::from(LANE_UNIT) > usable;
        let (tick_width, positions) = if scroll_mode {
            // Scroll mode: fixed native scale, viewport offsets the origin.
            self.lane_unit.set(LANE_UNIT);
            (1, lane_cluster_positions(lanes))
        } else {
            self.lane_unit.set(0);
            (
                u16::try_from((usable / u64::from(max_sequence).max(1)).clamp(1, 3)).unwrap_or(1),
                Vec::new(),
            )
        };
        let native_total = positions
            .last()
            .map_or(block_count, |last| last + 1 + LANE_CLUSTER_GAP);
        let mut lane_viewport = self.lane_viewport.get();
        lane_viewport.set_metrics(
            usize::try_from(native_total).unwrap_or(usize::MAX),
            usize::from(strip.width),
        );
        self.lane_viewport.set(lane_viewport);
        let mut rows: Vec<Vec<Rect>> = vec![Vec::new(), Vec::new()];
        let mut hits = Vec::new();
        for (lane_index, block_ids) in [model_blocks, tool_blocks].into_iter().enumerate() {
            for &block_index in &block_ids {
                // Cluster position on the strip axis in scroll mode; stream
                // position spreads blocks in fit mode.
                let ordinal = if positions.is_empty() {
                    u64::from(lanes.blocks[block_index].sequence)
                } else {
                    positions.get(block_index).copied().unwrap_or(u64::MAX)
                };
                // Native tick extent, clipped to the strip on both sides so
                // partially scrolled-out blocks still paint their visible part.
                let tick_start =
                    ordinal as i64 - u64::try_from(viewport_offset).unwrap_or(0) as i64;
                let visible_start = tick_start.max(0);
                let visible_end = tick_start + i64::from(tick_width);
                if visible_end <= 0 {
                    continue;
                }
                let x = strip
                    .x
                    .saturating_add(u16::try_from(visible_start).unwrap_or(u16::MAX));
                let width = u16::try_from(visible_end - visible_start)
                    .unwrap_or(0)
                    .min(strip.right().saturating_sub(x));
                if width == 0 {
                    continue;
                }
                let rect = Rect::new(x, strip.y + lane_index as u16, width, 1);
                rows[lane_index].push(rect);
                hits.push((rect, block_index));
            }
        }
        (Some((strip, rows)), hits)
    }
}

fn row_hit_rects(list: Rect, viewport: &ViewportState) -> Vec<(Rect, HitId)> {
    viewport
        .visible_range()
        .enumerate()
        .map(|(visible_index, row_index)| {
            (
                Rect::new(
                    list.x,
                    list.y + u16::try_from(visible_index).unwrap_or(u16::MAX),
                    list.width,
                    1,
                ),
                HitId::Row(row_index),
            )
        })
        .collect()
}

pub(super) fn short_agent_label(agent: &piko_protocol::HistoryAgentSummary) -> String {
    if agent.agent_spec_id.len() > 18 {
        crate::features::short_id(&agent.agent_spec_id)
    } else {
        agent.agent_spec_id.clone()
    }
}
