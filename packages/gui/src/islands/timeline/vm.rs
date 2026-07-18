//! Pure timeline view-model derived from Client Core projection.

use piko_client_core::{
    ClientState, CommittedItem, RealtimeDraft, TimelineItem, ToolItem, ToolStatus as CoreToolStatus,
};
use piko_protocol::messages::{ContentBlock, Message, MessageContent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineRowKind {
    User,
    Assistant,
    Tool,
    System,
    Streaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCardStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineRow {
    pub id: String,
    pub kind: TimelineRowKind,
    pub label: String,
    pub body: String,
    pub streaming: bool,
    /// Assistant rows use Markdown when needed for prose or thinking style.
    pub render_markdown: bool,
    pub tool_status: Option<ToolCardStatus>,
    /// Full detail for expandable tool cards (args / result JSON).
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineViewModel {
    pub rows: Vec<TimelineRow>,
    pub selected_agent_id: Option<String>,
    pub selected_agent_name: Option<String>,
}

pub fn derive_timeline(state: &ClientState) -> TimelineViewModel {
    let Some(session) = state.live_session.as_ref() else {
        return TimelineViewModel::default();
    };
    let Some(agent_id) = session.selected_agent.as_ref() else {
        return TimelineViewModel::default();
    };

    let name = session
        .agents
        .iter()
        .find(|a| &a.agent_instance_id == agent_id)
        .map(|a| a.name.clone());

    let rows = session
        .timelines
        .get(agent_id)
        .map(|tl| rows_from_items(tl.items()))
        .unwrap_or_default();

    TimelineViewModel {
        rows,
        selected_agent_id: Some(agent_id.clone()),
        selected_agent_name: name,
    }
}

fn rows_from_items(items: &[TimelineItem]) -> Vec<TimelineRow> {
    let mut result_by_call: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();
    for item in items {
        if let TimelineItem::Committed(c) = item
            && let Message::ToolResult {
                tool_call_id,
                is_error,
                ..
            } = &c.message
        {
            result_by_call.insert(tool_call_id.clone(), is_error == &Some(true));
        }
    }

    items
        .iter()
        .filter_map(|item| row_from_item(item, &result_by_call))
        .collect()
}

fn row_from_item(
    item: &TimelineItem,
    result_by_call: &std::collections::HashMap<String, bool>,
) -> Option<TimelineRow> {
    match item {
        TimelineItem::Committed(committed) => Some(row_from_committed(committed, result_by_call)),
        TimelineItem::RealtimeDraft(draft) => Some(row_from_draft(draft)),
        TimelineItem::Tool(tool) => Some(row_from_tool(tool)),
    }
}

fn row_from_tool(tool: &ToolItem) -> TimelineRow {
    let status = match tool.status {
        CoreToolStatus::Running => ToolCardStatus::Running,
        CoreToolStatus::Completed => ToolCardStatus::Completed,
        CoreToolStatus::Failed => ToolCardStatus::Failed,
    };
    let status_text = match status {
        ToolCardStatus::Running => "running",
        ToolCardStatus::Completed => "completed",
        ToolCardStatus::Failed => "failed",
    };
    let mut detail = format!(
        "Arguments\n{}",
        serde_json::to_string_pretty(&tool.args).unwrap_or_else(|_| tool.args.to_string())
    );
    if let Some(result) = &tool.result {
        detail.push_str("\n\nResult\n");
        detail
            .push_str(&serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string()));
    }
    TimelineRow {
        id: format!("tool-{}", tool.tool_call_id),
        kind: TimelineRowKind::Tool,
        label: format!("tool {}", tool.tool_name),
        body: format!("{} ({status_text})", tool.tool_name),
        streaming: false,
        render_markdown: false,
        tool_status: Some(status),
        detail: Some(detail),
    }
}

fn row_from_committed(
    item: &CommittedItem,
    result_by_call: &std::collections::HashMap<String, bool>,
) -> TimelineRow {
    let (kind, label, body, tool_status, detail) = message_parts(&item.message, result_by_call);
    let render_markdown = kind == TimelineRowKind::Assistant;
    TimelineRow {
        id: item.message_id.clone(),
        kind,
        label,
        body,
        streaming: false,
        render_markdown,
        tool_status,
        detail,
    }
}

fn row_from_draft(draft: &RealtimeDraft) -> TimelineRow {
    let body = draft.text_segments.join("");
    let thinking = draft.thinking_segments.join("");
    let has_thinking = !thinking.is_empty();
    let body = if !has_thinking {
        body
    } else if body.is_empty() {
        quote_markdown(&thinking)
    } else {
        format!("{}\n\n{body}", quote_markdown(&thinking))
    };
    TimelineRow {
        id: draft.message_id.clone(),
        kind: TimelineRowKind::Streaming,
        label: "assistant".into(),
        body,
        streaming: true,
        render_markdown: has_thinking,
        tool_status: None,
        detail: None,
    }
}

fn message_parts(
    message: &Message,
    result_by_call: &std::collections::HashMap<String, bool>,
) -> (
    TimelineRowKind,
    String,
    String,
    Option<ToolCardStatus>,
    Option<String>,
) {
    match message {
        Message::User { content, .. } => (
            TimelineRowKind::User,
            "user".into(),
            content_text(content),
            None,
            None,
        ),
        Message::Assistant { content, .. } => (
            TimelineRowKind::Assistant,
            "assistant".into(),
            assistant_markdown(content),
            None,
            None,
        ),
        Message::ToolCall {
            id,
            name,
            arguments,
            ..
        } => {
            let status = match result_by_call.get(id) {
                Some(true) => ToolCardStatus::Failed,
                Some(false) => ToolCardStatus::Completed,
                None => ToolCardStatus::Running,
            };
            let detail = Some(pretty_json(arguments));
            let body = match status {
                ToolCardStatus::Running => format!("{name} (running)"),
                ToolCardStatus::Completed => format!("{name} (completed)"),
                ToolCardStatus::Failed => format!("{name} (failed)"),
            };
            (
                TimelineRowKind::Tool,
                format!("tool {name}"),
                body,
                Some(status),
                detail,
            )
        }
        Message::ToolResult {
            tool_name,
            content,
            is_error,
            details,
            ..
        } => {
            let label = match tool_name {
                Some(n) => format!("result {n}"),
                None => "tool result".into(),
            };
            let status = if is_error == &Some(true) {
                ToolCardStatus::Failed
            } else {
                ToolCardStatus::Completed
            };
            let body = blocks_text(content);
            let body = if is_error == &Some(true) {
                format!("error: {body}")
            } else {
                body
            };
            let detail = details.as_ref().map(pretty_json).or_else(|| {
                let t = blocks_text(content);
                if t.is_empty() { None } else { Some(t) }
            });
            (TimelineRowKind::Tool, label, body, Some(status), detail)
        }
        Message::Context { content, .. } => (
            TimelineRowKind::System,
            "context".into(),
            content_text(content),
            None,
            None,
        ),
    }
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn content_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(s) => s.clone(),
        MessageContent::Blocks(blocks) => blocks_text(blocks),
    }
}

fn blocks_text(blocks: &[ContentBlock]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text { text } => parts.push(text.clone()),
            ContentBlock::Thinking { thinking, .. } => {
                parts.push(thinking.clone());
            }
            ContentBlock::Image { mime_type, .. } => {
                parts.push(format!("[image:{mime_type}]"));
            }
        }
    }
    parts.join("\n")
}

/// Prefer original text blocks as markdown source; thinking is an unlabeled blockquote.
fn assistant_markdown(blocks: &[ContentBlock]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text { text } => parts.push(text.clone()),
            ContentBlock::Thinking { thinking, .. } => {
                parts.push(quote_markdown(thinking));
            }
            ContentBlock::Image { mime_type, .. } => {
                parts.push(format!("*[image:{mime_type}]*"));
            }
        }
    }
    parts.join("\n\n")
}

fn quote_markdown(text: &str) -> String {
    text.lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
