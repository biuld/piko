use piko_protocol::ContentBlock as ProtocolContentBlock;

use crate::app::ToolStatus;

/// Timeline item accepted by the timeline feature reducer.
#[derive(Clone)]
pub enum TimelineEntry {
    #[allow(dead_code)]
    Tool(ToolEntry),
    Error(String),
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimelineKind {
    User,
    Assistant,
    Tool,
    SessionFact,
    Summary,
    CustomMessage,
    Error,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ComponentId {
    MessageId(String),
    ToolCallId(String),
    EntryId(String),
    Local(u64),
}

#[derive(Clone)]
pub enum TimelineComponent {
    User(UserMessageComponent),
    Assistant(AssistantMessageComponent),
    Tool(ToolEntry),
    SessionFact(SessionFactComponent),
    Summary(SummaryComponent),
    CustomMessage(CustomMessageComponent),
    Error(ErrorComponent),
}

#[cfg(test)]
impl TimelineComponent {
    pub fn kind(&self) -> TimelineKind {
        match self {
            Self::User(_) => TimelineKind::User,
            Self::Assistant(_) => TimelineKind::Assistant,
            Self::Tool(_) => TimelineKind::Tool,
            Self::SessionFact(_) => TimelineKind::SessionFact,
            Self::Summary(_) => TimelineKind::Summary,
            Self::CustomMessage(_) => TimelineKind::CustomMessage,
            Self::Error(_) => TimelineKind::Error,
        }
    }
}

impl TimelineComponent {
    pub fn id(&self) -> &ComponentId {
        match self {
            Self::User(component) => &component.id,
            Self::Assistant(component) => &component.id,
            Self::Tool(component) => &component.component_id,
            Self::SessionFact(component) => &component.id,
            Self::Summary(component) => &component.id,
            Self::CustomMessage(component) => &component.id,
            Self::Error(component) => &component.id,
        }
    }
}

#[derive(Clone)]
pub struct UserMessageComponent {
    #[allow(dead_code)]
    pub id: ComponentId,
    pub text: String,
    /// Epoch milliseconds from protocol `Message::User.timestamp`, when set.
    pub timestamp: Option<i64>,
}

#[derive(Clone)]
pub struct AssistantMessageComponent {
    #[allow(dead_code)]
    pub id: ComponentId,
    pub blocks: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    pub error_message: Option<String>,
    /// Epoch milliseconds from protocol `Message::Assistant.timestamp`, when set.
    pub timestamp: Option<i64>,
}

#[derive(Clone)]
pub enum ContentBlock {
    Text(String),
    Thinking(String),
    Image { mime_type: String },
}

impl From<ProtocolContentBlock> for ContentBlock {
    fn from(block: ProtocolContentBlock) -> Self {
        match block {
            ProtocolContentBlock::Text { text } => Self::Text(text),
            ProtocolContentBlock::Thinking { thinking, .. } => Self::Thinking(thinking),
            ProtocolContentBlock::Image { mime_type, .. } => Self::Image { mime_type },
            other => Self::Text(other.text_projection()),
        }
    }
}

#[derive(Clone)]
pub struct SessionFactComponent {
    #[allow(dead_code)]
    pub id: ComponentId,
    pub label: &'static str,
    pub text: String,
}

#[derive(Clone, Copy)]
pub enum SummaryKind {
    Compaction,
    Branch,
}

#[derive(Clone)]
pub struct SummaryComponent {
    #[allow(dead_code)]
    pub id: ComponentId,
    pub kind: SummaryKind,
    pub text: String,
}

#[derive(Clone)]
pub struct CustomMessageComponent {
    #[allow(dead_code)]
    pub id: ComponentId,
    pub custom_type: String,
    pub content: piko_protocol::CustomMessageContent,
}

#[derive(Clone)]
pub struct ErrorComponent {
    #[allow(dead_code)]
    pub id: ComponentId,
    pub text: String,
    /// Turn whose authored output this transient error terminates. Keeping the
    /// anchor lets projection rebuilds preserve causal order when host events
    /// from concurrent command streams arrive interleaved.
    pub after_turn_id: Option<String>,
}

/// Tool call state tracked inside the timeline.
#[derive(Clone)]
pub struct ToolEntry {
    pub component_id: ComponentId,
    pub id: String,
    pub name: String,
    pub status: ToolStatus,
    pub args: String,
    pub result: Option<String>,
    pub result_details: Option<String>,
    /// Parent assistant message id (projection fidelity; not shown in card chrome).
    #[allow(dead_code)]
    pub parent_message_id: Option<String>,
    /// Transient presentation state owned by this session's Timeline.
    pub expanded: bool,
}

impl ToolEntry {
    pub fn new(
        id: String,
        name: String,
        status: ToolStatus,
        args: String,
        result: Option<String>,
        parent_message_id: Option<String>,
    ) -> Self {
        Self {
            component_id: ComponentId::ToolCallId(id.clone()),
            id,
            name,
            status,
            args,
            result,
            result_details: None,
            parent_message_id,
            expanded: false,
        }
    }
}
