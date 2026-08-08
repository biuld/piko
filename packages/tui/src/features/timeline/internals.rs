use super::*;

impl Timeline {
    pub(super) fn append_assistant_block(
        &mut self,
        message_id: String,
        delta: String,
        kind: AssistantBlockKind,
    ) {
        if self
            .component_index(&ComponentId::MessageId(message_id.clone()))
            .is_none()
        {
            self.start_assistant(message_id.clone());
        }
        let id = ComponentId::MessageId(message_id);
        if let Some(TimelineComponent::Assistant(component)) = self.component_mut(&id) {
            match (component.blocks.last_mut(), kind) {
                (Some(ContentBlock::Text(text)), AssistantBlockKind::Text) => text.push_str(&delta),
                (Some(ContentBlock::Thinking(text)), AssistantBlockKind::Thinking) => {
                    text.push_str(&delta)
                }
                (_, AssistantBlockKind::Text) => {
                    component.blocks.push(ContentBlock::Text(delta));
                }
                (_, AssistantBlockKind::Thinking) => {
                    component.blocks.push(ContentBlock::Thinking(delta));
                }
            }
        }
    }

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

    pub(super) fn reorder_committed_messages(&mut self) {
        let seq = &self.committed_task_seq;
        self.components
            .make_contiguous()
            .sort_by_key(|component| seq.get(component.id()).copied().unwrap_or(u64::MAX));
    }

    pub(super) fn upsert_or_push(&mut self, component: TimelineComponent) {
        let id = component.id().clone();
        if let Some(index) = self.component_index(&id) {
            self.components[index] = component;
        } else {
            self.push_component(component);
        }
    }

    pub(super) fn component_index(&self, id: &ComponentId) -> Option<usize> {
        self.components
            .iter()
            .position(|component| component.id() == id)
    }

    pub(super) fn component_mut(&mut self, id: &ComponentId) -> Option<&mut TimelineComponent> {
        self.components
            .iter_mut()
            .find(|component| component.id() == id)
    }

    pub(super) fn local_id(&mut self) -> ComponentId {
        let id = self.next_local_id;
        self.next_local_id = self.next_local_id.saturating_add(1);
        ComponentId::Local(id)
    }
}
