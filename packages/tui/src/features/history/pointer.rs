use crate::ui::components::split_pane::PaneSide;
use piko_tui_layout::Component;
use ratatui::{Frame, layout::Rect};

use super::{HistoryCtx, HistoryPanel, HistoryRow};
use crate::{
    app::{HitId, command::SurfaceAction},
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture, paint_element_hover},
};

const WHEEL_STEP: isize = 3;

impl PointerComponent<HitId> for HistoryPanel {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Mode(index))) => {
                vec![SurfaceAction::HistorySelectAgent(index).into()]
            }
            (PointerGesture::Activate, Some(HitId::Lane(block_index))) => {
                self.select_lane_block(block_index);
                Vec::new()
            }
            (PointerGesture::Activate, Some(HitId::Tab(index))) => {
                self.select_detail_tab(index);
                Vec::new()
            }
            (PointerGesture::Activate, Some(HitId::Row(index))) => {
                if index < self.row_count() {
                    self.active_pane = PaneSide::First;
                    self.selected = index;
                    self.reveal_lane_block();
                    vec![SurfaceAction::Confirm.into()]
                } else {
                    Vec::new()
                }
            }
            (PointerGesture::Activate, Some(HitId::Content)) => {
                self.active_pane = PaneSide::Second;
                Vec::new()
            }
            (PointerGesture::ScrollDown, element) => {
                self.scroll_pane(element, hit.x, hit.y, WHEEL_STEP);
                Vec::new()
            }
            (PointerGesture::ScrollUp, element) => {
                self.scroll_pane(element, hit.x, hit.y, -WHEEL_STEP);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

impl HistoryPanel {
    fn scroll_pane(&mut self, element: Option<HitId>, x: u16, y: u16, delta: isize) {
        let side = self.painted_split.get().and_then(|plan| plan.pane_at(x, y));
        let element = match side {
            Some(PaneSide::First) => Some(HitId::Row(self.selected)),
            Some(PaneSide::Second) => Some(HitId::Content),
            None => element,
        };
        match element {
            Some(HitId::Content) => self.detail_viewport.get_mut().scroll_by(delta),
            Some(HitId::Lane(_)) | None if self.over_lane_strip(y) => {
                let unit = self.lane_unit.get().max(1);
                self.lane_viewport
                    .get_mut()
                    .scroll_by(delta * i64::from(unit) as isize);
            }
            Some(HitId::Row(_)) => {
                let mut viewport = self.viewport.get();
                viewport.scroll_by(delta);
                let visible = viewport.visible_range();
                if !visible.is_empty() {
                    self.selected = self.selected.clamp(visible.start, visible.end - 1);
                }
                self.viewport.set(viewport);
                self.reveal_lane_block();
            }
            _ => {}
        }
    }

    /// Whether the pointer sits over the lane strip (blocks or the shared
    /// axis area between them).
    fn over_lane_strip(&self, y: u16) -> bool {
        self.lane_strip_rect
            .get()
            .is_some_and(|strip| strip.y <= y && y < strip.bottom())
    }

    /// Scroll the lane strip so the selected row's block is on the strip.
    pub(crate) fn reveal_lane_block(&self) {
        let unit = self.lane_unit.get();
        if unit == 0 {
            return;
        }
        let Some(lanes) = &self.lanes else {
            return;
        };
        let Some(position) = self.stream_item_at(self.selected).and_then(|item| {
            lanes
                .blocks
                .iter()
                .position(|block| block.reference.token == item.item_ref.token)
        }) else {
            return;
        };
        let positions = super::layout::lane_cluster_positions(lanes);
        let Some(start) = positions.get(position).copied() else {
            return;
        };
        let mut viewport = self.lane_viewport.get();
        let start = usize::try_from(start).unwrap_or(0);
        viewport.ensure_visible(start..start + 1);
        self.lane_viewport.set(viewport);
    }

    /// Select the stream row that a lane block refers to.
    pub(crate) fn select_lane_block(&mut self, block_index: usize) {
        let Some(token) = self
            .lanes
            .as_ref()
            .and_then(|lanes| lanes.blocks.get(block_index))
            .map(|block| block.reference.token.clone())
        else {
            return;
        };
        let rows = self.visible_rows();
        if let Some(position) = rows.iter().position(|row| match row {
            HistoryRow::Stream(item) => item.item_ref.token == token,
            _ => false,
        }) {
            self.selected = position;
            self.viewport
                .get_mut()
                .ensure_visible(self.selected..self.selected.saturating_add(1));
            self.active_pane = PaneSide::First;
            self.reveal_lane_block();
        }
    }

    pub fn paint_hover(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &HistoryCtx<'_>,
        interaction: piko_tui_layout::InteractionState<HitId>,
    ) {
        let regions = Component::<HitId, HistoryCtx<'_>>::component_regions(self, area);
        paint_element_hover(
            frame,
            &regions,
            interaction,
            Some(HitId::Content),
            ctx.theme,
        );
    }
}
