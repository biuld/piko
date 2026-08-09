use super::AgentPanelState;
use crate::{
    app::{HitId, command::SurfaceAction},
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
};

impl PointerComponent<HitId> for AgentPanelState {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Row(i)))
                if !self.is_loading() && i < self.list.len() =>
            {
                self.list.selected = i;
                vec![SurfaceAction::Confirm.into()]
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
