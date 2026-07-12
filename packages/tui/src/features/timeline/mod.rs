use std::collections::{HashMap, VecDeque};

use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::{Message, RealtimeMessageEvent, TranscriptCommittedEvent};

mod component;
mod markdown;
mod render;
mod viewport;

#[cfg(test)]
pub use component::TimelineKind;
pub use component::{
    AssistantMessageComponent, ComponentId, ContentBlock, ErrorComponent, NoticeColor,
    NoticeComponent, TimelineComponent, TimelineEntry, ToolEntry, UserMessageComponent,
};
pub use viewport::ScrollViewport;

const MAX_COMPONENTS: usize = 500;

/// In-memory component stream plus viewport/presentation state.
pub struct Timeline {
    pub components: VecDeque<TimelineComponent>,
    pub viewport: ScrollViewport,
    pub tools_expanded: bool,
    pub thinking_visible: bool,
    /// Running and completed tool calls, kept for status lookup.
    pub tool_calls: Vec<ToolEntry>,
    live_assistant: Option<ComponentId>,
    next_local_id: u64,
    committed_messages: HashMap<String, (u64, Message)>,
    committed_task_seq: HashMap<ComponentId, u64>,
    realtime_delta_seq: HashMap<String, u64>,
}

impl Timeline {
    pub fn new() -> Self {
        Self {
            components: VecDeque::new(),
            viewport: ScrollViewport::default(),
            tools_expanded: false,
            thinking_visible: true,
            tool_calls: Vec::new(),
            live_assistant: None,
            next_local_id: 1,
            committed_messages: HashMap::new(),
            committed_task_seq: HashMap::new(),
            realtime_delta_seq: HashMap::new(),
        }
    }

    pub fn push(&mut self, entry: TimelineEntry) {
        match entry {
            TimelineEntry::System(text) => self.push_notice("system", text, NoticeColor::System),
            TimelineEntry::Tool(tool) => {
                let updated = self.upsert_tool(tool.clone());
                if !updated {
                    self.push_component(TimelineComponent::Tool(tool));
                }
            }
            TimelineEntry::Session(text) => self.push_notice("session", text, NoticeColor::Session),
            TimelineEntry::Error(text) => self.push_error(text),
        }
    }

    pub fn push_user(&mut self, message_id: String, text: String) {
        let id = ComponentId::MessageId(message_id);
        self.upsert_or_push(TimelineComponent::User(UserMessageComponent { id, text }));
    }

    pub fn start_assistant(&mut self, message_id: String) {
        let id = ComponentId::MessageId(message_id);
        if self.component_index(&id).is_none() {
            self.push_component(TimelineComponent::Assistant(AssistantMessageComponent {
                id: id.clone(),
                blocks: Vec::new(),
                stop_reason: None,
                error_message: None,
            }));
        }
        self.live_assistant = Some(id);
    }

    pub fn append_text_delta(&mut self, message_id: String, delta: String) {
        self.append_assistant_block(message_id, delta, AssistantBlockKind::Text);
    }

    pub fn append_thinking_delta(&mut self, message_id: String, delta: String) {
        self.append_assistant_block(message_id, delta, AssistantBlockKind::Thinking);
    }

    pub fn end_assistant_draft(
        &mut self,
        message_id: String,
        stop_reason: Option<String>,
        error_message: Option<String>,
    ) {
        let id = ComponentId::MessageId(message_id);
        if let Some(TimelineComponent::Assistant(component)) = self.component_mut(&id) {
            component.stop_reason = stop_reason;
            component.error_message = error_message;
        }
        if self.live_assistant.as_ref() == Some(&id) {
            self.live_assistant = None;
        }
    }

    pub fn apply_realtime(&mut self, event: RealtimeMessageEvent) {
        if self.committed_messages.contains_key(&event.message_id) {
            return;
        }
        if self
            .realtime_delta_seq
            .get(&event.message_id)
            .is_some_and(|seq| *seq >= event.delta_seq)
        {
            return;
        }
        self.realtime_delta_seq
            .insert(event.message_id.clone(), event.delta_seq);
        match event.delta {
            RealtimeDelta::MessageStarted { role } => {
                if matches!(role, piko_protocol::MessageRole::Assistant) {
                    self.start_assistant(event.message_id);
                }
            }
            RealtimeDelta::Text { delta, .. } => {
                self.append_text_delta(event.message_id, delta);
            }
            RealtimeDelta::Thinking { delta, .. } => {
                self.append_thinking_delta(event.message_id, delta);
            }
            RealtimeDelta::ToolCall { .. } => {}
            RealtimeDelta::MessageEnded {
                stop_reason,
                error_message,
            } => self.end_assistant_draft(event.message_id, stop_reason, error_message),
        }
    }

    pub fn apply_committed(&mut self, event: TranscriptCommittedEvent) -> bool {
        if let Some((task_seq, message)) = self.committed_messages.get(&event.message_id) {
            return *task_seq == event.transcript_seq && *message == event.message;
        }
        let message_id = event.message_id.clone();
        let message = event.message.clone();
        match &message {
            Message::User { .. } => {
                let text = crate::text::message_to_text(&message);
                self.push_user(message_id.clone(), text);
                self.committed_task_seq.insert(
                    ComponentId::MessageId(message_id.clone()),
                    event.transcript_seq,
                );
            }
            Message::Assistant { .. } => {
                self.complete_assistant_message(message_id.clone(), message.clone());
                self.committed_task_seq.insert(
                    ComponentId::MessageId(message_id.clone()),
                    event.transcript_seq,
                );
            }
            Message::ToolCall {
                id,
                name,
                arguments,
                ..
            } => {
                self.push(TimelineEntry::Tool(ToolEntry::new(
                    id.clone(),
                    name.clone(),
                    crate::app::ToolStatus::Running,
                    crate::text::compact_json(arguments),
                    None,
                    None,
                )));
                self.committed_task_seq
                    .entry(ComponentId::ToolCallId(id.clone()))
                    .or_insert(event.transcript_seq);
            }
            Message::ToolResult {
                tool_call_id,
                tool_name,
                is_error,
                ..
            } => {
                let text = crate::text::message_to_text(&message);
                let status = if is_error.unwrap_or(false) {
                    crate::app::ToolStatus::Failed
                } else {
                    crate::app::ToolStatus::Completed
                };
                let mut tool = self
                    .tool_calls
                    .iter()
                    .find(|tool| tool.id == *tool_call_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        ToolEntry::new(
                            tool_call_id.clone(),
                            tool_name.clone().unwrap_or_else(|| "tool".into()),
                            status,
                            String::new(),
                            None,
                            None,
                        )
                    });
                tool.status = status;
                tool.result = Some(text);
                self.push(TimelineEntry::Tool(tool));
                self.committed_task_seq
                    .entry(ComponentId::ToolCallId(tool_call_id.clone()))
                    .or_insert(event.transcript_seq);
            }
        }
        self.committed_messages
            .insert(message_id, (event.transcript_seq, event.message));
        self.reorder_committed_messages();
        true
    }

    pub fn complete_assistant_message(&mut self, message_id: String, message: Message) {
        let Message::Assistant {
            content,
            stop_reason,
            error_message,
            ..
        } = message
        else {
            return;
        };
        let id = ComponentId::MessageId(message_id);
        let blocks = content.into_iter().map(ContentBlock::from).collect();
        let component = TimelineComponent::Assistant(AssistantMessageComponent {
            id: id.clone(),
            blocks,
            stop_reason,
            error_message,
        });
        self.upsert_or_push(component);
        if self.live_assistant.as_ref() == Some(&id) {
            self.live_assistant = None;
        }
    }

    pub fn push_notice(&mut self, label: &'static str, text: String, color: NoticeColor) {
        let id = self.local_id();
        self.push_component(TimelineComponent::Notice(NoticeComponent {
            id,
            label,
            text,
            color,
        }));
    }

    pub fn push_error(&mut self, text: String) {
        let id = self.local_id();
        self.push_component(TimelineComponent::Error(ErrorComponent { id, text }));
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.viewport.scroll_up(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.viewport.scroll_down(amount);
    }

    pub fn jump_latest(&mut self) {
        self.viewport.jump_latest();
    }

    pub fn clear(&mut self) {
        self.components.clear();
        self.tool_calls.clear();
        self.viewport.jump_latest();
        self.live_assistant = None;
        self.committed_messages.clear();
        self.committed_task_seq.clear();
        self.realtime_delta_seq.clear();
    }

    /// Update or insert a tool in the registry. Returns `true` if an existing
    /// visible component was found and updated in-place.
    pub fn upsert_tool(&mut self, mut tool: ToolEntry) -> bool {
        tool.component_id = ComponentId::ToolCallId(tool.id.clone());
        if let Some(existing) = self.tool_calls.iter_mut().find(|t| t.id == tool.id) {
            *existing = tool.clone();
        } else {
            self.tool_calls.push(tool.clone());
        }
        for component in self.components.iter_mut().rev() {
            if let TimelineComponent::Tool(existing) = component
                && existing.id == tool.id
            {
                *existing = tool;
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    pub fn tool_call_count(&self) -> usize {
        self.components
            .iter()
            .filter(|component| matches!(component, TimelineComponent::Tool(_)))
            .count()
    }

    #[cfg(test)]
    pub fn component_kinds(&self) -> Vec<TimelineKind> {
        self.components
            .iter()
            .map(TimelineComponent::kind)
            .collect()
    }

    #[cfg(test)]
    pub fn message_ids(&self) -> Vec<String> {
        self.components
            .iter()
            .filter_map(|component| match component.id() {
                ComponentId::MessageId(id) => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    #[cfg(test)]
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

    fn append_assistant_block(
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

    fn push_component(&mut self, component: TimelineComponent) {
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

    fn reorder_committed_messages(&mut self) {
        let seq = &self.committed_task_seq;
        self.components
            .make_contiguous()
            .sort_by_key(|component| seq.get(component.id()).copied().unwrap_or(u64::MAX));
    }

    fn upsert_or_push(&mut self, component: TimelineComponent) {
        let id = component.id().clone();
        if let Some(index) = self.component_index(&id) {
            self.components[index] = component;
        } else {
            self.push_component(component);
        }
    }

    fn component_index(&self, id: &ComponentId) -> Option<usize> {
        self.components
            .iter()
            .position(|component| component.id() == id)
    }

    fn component_mut(&mut self, id: &ComponentId) -> Option<&mut TimelineComponent> {
        self.components
            .iter_mut()
            .find(|component| component.id() == id)
    }

    fn local_id(&mut self) -> ComponentId {
        let id = self.next_local_id;
        self.next_local_id = self.next_local_id.saturating_add(1);
        ComponentId::Local(id)
    }
}

enum AssistantBlockKind {
    Text,
    Thinking,
}
