use piko_tui_layout::{Component, InteractionState, SurfacePanel};
use ratatui::{Frame, layout::Rect};

use super::{SessionList, SessionListCtx, SessionScope};
use crate::{
    app::{
        HitId,
        command::{SessionAction, SurfaceAction},
    },
    navigation::SurfaceId,
    ui::{
        components::selectable_list::paint_row_hover,
        interaction::{ComponentHit, PointerComponent, PointerGesture, paint_element_hover},
    },
};

impl Component<HitId, SessionListCtx<'_>> for SessionList {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &SessionListCtx<'_>) {
        self.render(
            frame,
            area,
            ctx.active_session_id,
            ctx.theme,
            ctx.tip,
            ctx.hints,
        );
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &SessionListCtx<'_>,
        interaction: InteractionState<HitId>,
    ) {
        self.render(
            frame,
            area,
            ctx.active_session_id,
            ctx.theme,
            ctx.tip,
            ctx.hints,
        );
        paint_row_hover(
            frame,
            &self.row_regions(area),
            interaction,
            self.list.selected,
            ctx.theme,
        );
        paint_element_hover(
            frame,
            &self.title_regions(area),
            interaction,
            Some(HitId::Mode(usize::from(self.scope == SessionScope::All))),
            ctx.theme,
        );
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let mut regions: Vec<_> = self
            .row_regions(area)
            .into_iter()
            .map(|(rect, i)| (rect, HitId::Row(i)))
            .collect();
        regions.extend(self.title_regions(area));
        regions
    }
}

impl PointerComponent<HitId> for SessionList {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Row(i))) if i < self.list.len() => {
                self.list.selected = i;
                vec![SurfaceAction::Confirm.into()]
            }
            (PointerGesture::Activate, Some(HitId::Mode(i))) => {
                let target = if i == 0 {
                    SessionScope::CurrentFolder
                } else {
                    SessionScope::All
                };
                if self.scope != target {
                    vec![SessionAction::ToggleScope.into()]
                } else {
                    Vec::new()
                }
            }
            (PointerGesture::ScrollUp, _) => {
                self.select_prev();
                Vec::new()
            }
            (PointerGesture::ScrollDown, _) => {
                self.select_next();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

impl SurfacePanel<SurfaceId, HitId, SessionListCtx<'_>> for SessionList {
    fn region(&self) -> SurfaceId {
        SurfaceId::Sessions
    }
}
