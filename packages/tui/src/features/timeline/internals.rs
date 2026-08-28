use super::*;

impl Timeline {
    pub(super) fn push_component(&mut self, component: TimelineComponent) {
        match &component {
            TimelineComponent::Tool(tool) => {
                self.intern_tool_id(&tool.id);
            }
            TimelineComponent::Thought(thought) => {
                self.intern_thought_key(thought.key.clone());
            }
            _ => {}
        }
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
        self.bump_layout_epoch();
    }

    pub(super) fn local_id(&mut self) -> ComponentId {
        let id = self.next_local_id;
        self.next_local_id = self.next_local_id.saturating_add(1);
        ComponentId::Local(id)
    }

    /// Stable per-tool-call hit identity. Ids are never reused (`next_hit_id`
    /// is monotonic), so a hit resolved from a slightly older frame can never
    /// alias a different tool.
    pub(super) fn intern_tool_id(&mut self, tool_call_id: &str) -> u64 {
        *self
            .hit_ids
            .entry(tool_call_id.to_string())
            .or_insert_with(|| {
                let id = self.next_hit_id;
                self.next_hit_id = self.next_hit_id.saturating_add(1);
                id
            })
    }

    pub(super) fn intern_thought_key(&mut self, key: ThoughtKey) -> u64 {
        *self.thought_hit_ids.entry(key).or_insert_with(|| {
            let id = self.next_hit_id;
            self.next_hit_id = self.next_hit_id.saturating_add(1);
            id
        })
    }

    pub fn thought(&self, key: &ThoughtKey) -> Option<ThoughtComponent> {
        self.components
            .iter()
            .find_map(|component| match component {
                TimelineComponent::Thought(thought) if &thought.key == key => Some(thought.clone()),
                _ => None,
            })
    }

    pub fn thought_key_for_hit(&self, hit_id: u64) -> Option<ThoughtKey> {
        self.thought_hit_ids
            .iter()
            .find_map(|(key, id)| (*id == hit_id).then(|| key.clone()))
    }

    #[allow(dead_code)]
    pub fn thought_hit_id(&self, key: &ThoughtKey) -> Option<u64> {
        self.thought_hit_ids.get(key).copied()
    }

    pub(super) fn bump_layout_epoch(&mut self) {
        self.layout_epoch = self.layout_epoch.saturating_add(1);
    }

    /// Layout-plan version; the pointer path recomputes a retained plan only
    /// when this differs from the plan's own epoch. Pure scroll never bumps it.
    pub(crate) fn layout_epoch(&self) -> u64 {
        self.layout_epoch
    }

    pub fn set_thinking_visible(&mut self, visible: bool) {
        self.thinking_visible = visible;
        self.bump_layout_epoch();
    }

    /// Toggle one tool block by its stable interned hit id. Resolving by id
    /// (not component slot) keeps clicks correct across rebuilds and scrolls.
    pub fn toggle_tool(&mut self, hit_id: u64) {
        let Some(tool_call_id) = self
            .hit_ids
            .iter()
            .find(|(_, id)| **id == hit_id)
            .map(|(tool_call_id, _)| tool_call_id.clone())
        else {
            return;
        };
        let Some(TimelineComponent::Tool(tool)) = self.components.iter_mut().find(
            |component| matches!(component, TimelineComponent::Tool(t) if t.id == tool_call_id),
        ) else {
            return;
        };
        tool.expanded = !tool.expanded;
        let expanded = tool.expanded;
        if let Some(registry) = self
            .tool_calls
            .iter_mut()
            .find(|tool| tool.id == tool_call_id)
        {
            registry.expanded = expanded;
        }
        self.bump_layout_epoch();
    }
}
