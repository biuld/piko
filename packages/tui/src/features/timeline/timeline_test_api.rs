use super::*;

impl Timeline {
    pub fn push_session_fact(&mut self, entry_id: String, label: &'static str, text: String) {
        self.push_component(TimelineComponent::SessionFact(SessionFactComponent {
            id: ComponentId::EntryId(entry_id),
            label,
            text,
        }));
    }

    pub fn tool_call_count(&self) -> usize {
        self.components
            .iter()
            .filter(|component| matches!(component, TimelineComponent::Tool(_)))
            .count()
    }

    pub fn tool_expanded(&self, tool_call_id: &str) -> Option<bool> {
        self.components
            .iter()
            .find_map(|component| match component {
                TimelineComponent::Tool(tool) if tool.id == tool_call_id => Some(tool.expanded),
                _ => None,
            })
    }

    pub fn component_kinds(&self) -> Vec<TimelineKind> {
        self.components
            .iter()
            .map(TimelineComponent::kind)
            .collect()
    }

    pub fn message_ids(&self) -> Vec<String> {
        self.components
            .iter()
            .filter_map(|component| match component.id() {
                ComponentId::MessageId(id) => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn assistant_text(&self, message_id: &str) -> Option<String> {
        self.components.iter().find_map(|component| {
            let TimelineComponent::Assistant(assistant) = component else {
                return None;
            };
            if assistant.id != ComponentId::MessageId(message_id.to_string()) {
                return None;
            }
            Some(
                assistant
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .collect(),
            )
        })
    }
}
