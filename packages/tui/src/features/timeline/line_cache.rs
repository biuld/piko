use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use ratatui::text::Line;

use crate::theme::Theme;

use super::ComponentId;
use super::component::{ContentBlock, TimelineComponent};
use super::render::component_lines;

#[derive(Clone, Eq, Hash, PartialEq)]
struct CacheKey {
    id: ComponentId,
    fingerprint: u64,
    hovered: bool,
}

#[derive(Default)]
pub(super) struct LineCache {
    width: u16,
    thinking_visible: bool,
    theme_name: String,
    entries: HashMap<CacheKey, Vec<Line<'static>>>,
}

impl LineCache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Format one component, reusing last frame's lines when identity, content,
    /// hover, width, thinking visibility, and theme name are unchanged.
    pub(super) fn lines_for(
        &mut self,
        component: &TimelineComponent,
        thinking_visible: bool,
        hovered: bool,
        theme: &Theme,
        width: u16,
    ) -> Vec<Line<'static>> {
        if self.width != width
            || self.thinking_visible != thinking_visible
            || self.theme_name != theme.name
        {
            self.entries.clear();
            self.width = width;
            self.thinking_visible = thinking_visible;
            self.theme_name = theme.name.clone();
        }
        let key = CacheKey {
            id: component.id().clone(),
            fingerprint: component_fingerprint(component),
            hovered,
        };
        if let Some(lines) = self.entries.get(&key) {
            return lines.clone();
        }
        let lines = component_lines(component, thinking_visible, hovered, theme, width);
        self.entries.insert(key, lines.clone());
        lines
    }

    /// Drop entries that were not used this frame so replaced drafts do not leak.
    pub(super) fn retain_ids(&mut self, ids: &[ComponentId]) {
        self.entries
            .retain(|key, _| ids.iter().any(|id| id == &key.id));
    }
}

fn component_fingerprint(component: &TimelineComponent) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match component {
        TimelineComponent::User(component) => {
            0u8.hash(&mut hasher);
            component.text.hash(&mut hasher);
            component.timestamp.hash(&mut hasher);
        }
        TimelineComponent::Assistant(component) => {
            1u8.hash(&mut hasher);
            for block in &component.blocks {
                match block {
                    ContentBlock::Text(text) => {
                        0u8.hash(&mut hasher);
                        text.hash(&mut hasher);
                    }
                    ContentBlock::Thinking(text) => {
                        1u8.hash(&mut hasher);
                        text.hash(&mut hasher);
                    }
                    ContentBlock::Image { mime_type } => {
                        2u8.hash(&mut hasher);
                        mime_type.hash(&mut hasher);
                    }
                }
            }
            component.stop_reason.hash(&mut hasher);
            component.error_message.hash(&mut hasher);
            component.timestamp.hash(&mut hasher);
        }
        TimelineComponent::Tool(tool) => {
            2u8.hash(&mut hasher);
            tool.id.hash(&mut hasher);
            tool.name.hash(&mut hasher);
            tool.status.hash(&mut hasher);
            tool.args.hash(&mut hasher);
            tool.result.hash(&mut hasher);
            tool.result_details.hash(&mut hasher);
            tool.expanded.hash(&mut hasher);
        }
        TimelineComponent::SessionFact(component) => {
            3u8.hash(&mut hasher);
            component.label.hash(&mut hasher);
            component.text.hash(&mut hasher);
        }
        TimelineComponent::Summary(component) => {
            4u8.hash(&mut hasher);
            (component.kind as u8).hash(&mut hasher);
            component.text.hash(&mut hasher);
        }
        TimelineComponent::CustomMessage(component) => {
            5u8.hash(&mut hasher);
            component.custom_type.hash(&mut hasher);
            hash_custom_content(&component.content, &mut hasher);
        }
        TimelineComponent::Error(component) => {
            6u8.hash(&mut hasher);
            component.text.hash(&mut hasher);
            component.after_turn_id.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn hash_custom_content(content: &piko_protocol::CustomMessageContent, hasher: &mut impl Hasher) {
    match content {
        piko_protocol::CustomMessageContent::String(text) => {
            0u8.hash(hasher);
            text.hash(hasher);
        }
        piko_protocol::CustomMessageContent::Blocks(blocks) => {
            1u8.hash(hasher);
            for block in blocks {
                match block {
                    piko_protocol::ContentBlock::Text { text } => {
                        0u8.hash(hasher);
                        text.hash(hasher);
                    }
                    piko_protocol::ContentBlock::Thinking { thinking, .. } => {
                        1u8.hash(hasher);
                        thinking.hash(hasher);
                    }
                    piko_protocol::ContentBlock::Image { mime_type, .. } => {
                        2u8.hash(hasher);
                        mime_type.hash(hasher);
                    }
                    other => {
                        3u8.hash(hasher);
                        other.text_projection().hash(hasher);
                    }
                }
            }
        }
    }
}
