use std::collections::{HashMap, VecDeque};

use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::{Message, TranscriptCommittedEvent};

mod component;
mod highlight;
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

mod internals;
mod timeline_impl;

enum AssistantBlockKind {
    Text,
    Thinking,
}
