use piko_protocol::ContentBlock as ProtocolContentBlock;

use crate::app::ToolStatus;

/// Timeline item accepted by the timeline feature reducer.
#[derive(Clone)]
pub enum TimelineEntry {
    System(String),
    Tool(ToolEntry),
    Session(String),
    Error(String),
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimelineKind {
    User,
    Assistant,
    Tool,
    Notice,
    Error,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ComponentId {
    MessageId(String),
    ToolCallId(String),
    Local(u64),
}

#[derive(Clone)]
pub enum TimelineComponent {
    User(UserMessageComponent),
    Assistant(AssistantMessageComponent),
    Tool(ToolEntry),
    Notice(NoticeComponent),
    Error(ErrorComponent),
}

#[cfg(test)]
impl TimelineComponent {
    pub fn kind(&self) -> TimelineKind {
        match self {
            Self::User(_) => TimelineKind::User,
            Self::Assistant(_) => TimelineKind::Assistant,
            Self::Tool(_) => TimelineKind::Tool,
            Self::Notice(_) => TimelineKind::Notice,
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
            Self::Notice(component) => &component.id,
            Self::Error(component) => &component.id,
        }
    }
}

#[derive(Clone)]
pub struct UserMessageComponent {
    pub id: ComponentId,
    pub text: String,
}

#[derive(Clone)]
pub struct AssistantMessageComponent {
    pub id: ComponentId,
    pub blocks: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    pub error_message: Option<String>,
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
        }
    }
}

#[derive(Clone)]
pub struct NoticeComponent {
    pub id: ComponentId,
    pub label: &'static str,
    pub text: String,
    pub color: NoticeColor,
}

#[derive(Clone, Copy)]
pub enum NoticeColor {
    System,
    Session,
}

#[derive(Clone)]
pub struct ErrorComponent {
    pub id: ComponentId,
    pub text: String,
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
    pub parent_message_id: Option<String>,
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
            parent_message_id,
        }
    }
}
