use super::*;

impl Timeline {
    pub(super) fn push_component(&mut self, component: TimelineComponent) {
        let is_at_bottom = self.viewport.is_at_latest();
        self.components.push_back(component);
        if is_at_bottom {
            self.viewport.jump_latest();
        } else {
            self.viewport.mark_appended();
        }
        while self.components.len() > MAX_COMPONENTS {
            self.components.pop_front();
        }
    }

    pub(super) fn local_id(&mut self) -> ComponentId {
        let id = self.next_local_id;
        self.next_local_id = self.next_local_id.saturating_add(1);
        ComponentId::Local(id)
    }
}
