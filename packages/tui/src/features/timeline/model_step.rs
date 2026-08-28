use piko_client_core::timeline::TimelineItem;
use std::collections::HashMap;

use super::{ComponentId, ModelStepBoundary, ModelStepDividerComponent, TimelineComponent};

pub(super) struct ModelStepLookup {
    assistant_messages: HashMap<String, (String, u32)>,
    tool_calls: HashMap<String, (String, u32)>,
}

impl ModelStepLookup {
    pub(super) fn for_item(&self, item: &TimelineItem) -> Option<(String, u32)> {
        match item {
            TimelineItem::Committed(message) => {
                self.assistant_messages.get(&message.message_id).cloned()
            }
            TimelineItem::RealtimeDraft(draft) => {
                self.assistant_messages.get(&draft.message_id).cloned()
            }
            TimelineItem::Tool(tool) => self.tool_calls.get(&tool.tool_call_id).cloned(),
            TimelineItem::SessionEntry(_) => None,
        }
    }
}

impl super::Timeline {
    pub fn apply_model_step_committed(
        &mut self,
        boundary: ModelStepBoundary,
    ) -> piko_client_core::ApplyOutcome {
        let outcome = self.projection.apply_model_step_committed(boundary.clone());
        if outcome == piko_client_core::ApplyOutcome::Applied {
            self.model_step_boundaries.push(boundary);
            self.mark_projection_applied();
        }
        outcome
    }

    pub(super) fn clear_model_step_state(&mut self) {
        self.tool_message_ids.clear();
        self.model_step_boundaries.clear();
    }

    pub(super) fn model_step_lookup(&self) -> ModelStepLookup {
        let mut lookup = ModelStepLookup {
            assistant_messages: HashMap::new(),
            tool_calls: HashMap::new(),
        };
        for boundary in &self.model_step_boundaries {
            let anchor = (boundary.model_step_id.clone(), boundary.step_index);
            lookup
                .assistant_messages
                .insert(boundary.assistant_message_id.clone(), anchor.clone());
            for message_id in &boundary.tool_call_message_ids {
                if let Some(call_id) = self.tool_message_ids.get(message_id) {
                    lookup.tool_calls.insert(call_id.clone(), anchor.clone());
                }
            }
        }
        lookup
    }

    pub(super) fn discard_leading_model_step_dividers(&mut self) {
        while matches!(
            self.components.front(),
            Some(TimelineComponent::ModelStepDivider(_))
        ) {
            self.components.pop_front();
        }
    }

    pub(super) fn model_step_divider(model_step_id: String, step_index: u32) -> TimelineComponent {
        TimelineComponent::ModelStepDivider(ModelStepDividerComponent {
            id: ComponentId::ModelStepId(model_step_id),
            step_index,
        })
    }
}
