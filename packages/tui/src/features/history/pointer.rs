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
            (PointerGesture::Activate, Some(HitId::Row(index))) => {
                if index < self.row_count() {
                    self.selected = index;
                    vec![SurfaceAction::Confirm.into()]
                } else {
                    Vec::new()
                }
            }
            (PointerGesture::ScrollDown, _) => {
                self.select_next();
                Vec::new()
            }
            (PointerGesture::ScrollUp, _) => {
                self.select_prev();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

impl HistoryPanel {
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
