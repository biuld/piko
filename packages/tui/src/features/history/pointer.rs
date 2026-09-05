use crate::ui::components::split_pane::PaneSide;
use piko_tui_layout::Component;
use ratatui::{Frame, layout::Rect};

use super::{HistoryCtx, HistoryPanel};
use crate::{
    app::{HitId, command::SurfaceAction},
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture, paint_element_hover},
};

impl PointerComponent<HitId> for HistoryPanel {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Mode(index))) => {
                vec![SurfaceAction::HistorySelectLens(index).into()]
            }
            (PointerGesture::Activate, Some(HitId::Inspect(index))) => {
                self.selected = index.min(self.row_count().saturating_sub(1));
                vec![SurfaceAction::HistoryInspect.into()]
            }
            (PointerGesture::Activate, Some(HitId::Row(index))) => {
                if index < self.row_count() {
                    self.active_pane = PaneSide::First;
                    self.selected = index;
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
                self.scroll_pane(element, hit.x, hit.y, 1);
                Vec::new()
            }
            (PointerGesture::ScrollUp, element) => {
                self.scroll_pane(element, hit.x, hit.y, -1);
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
            Some(HitId::Row(_)) => {
                self.selected = self
                    .selected
                    .saturating_add_signed(delta)
                    .min(self.row_count().saturating_sub(1));
            }
            _ => {}
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
