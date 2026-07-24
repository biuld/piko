//! Pure timeline view-model derived from Client Core projection.
//!
//! Chat-primary projection: tools are one compact row (call+result merged),
//! thinking is a separate always-visible muted payload, ToolResult rows are
//! not shown. Render follows timeline item order from Client Core.

use std::collections::{HashMap, HashSet};

use piko_client_core::{
    ClientState, CommittedItem, RealtimeDraft, TimelineItem, ToolItem, ToolStatus as CoreToolStatus,
};
use piko_protocol::messages::{ContentBlock, Message, MessageContent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimelineRowKind {
    User,
    Assistant,
    Tool,
    System,
    Streaming,
}

/// Chat-style speaker for consecutive-message grouping.
/// Tools belong to the Assistant side; system rows break groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualSender {
    You,
    Assistant,
    System,
}

impl TimelineRowKind {
    pub fn visual_sender(self) -> VisualSender {
        match self {
            Self::User => VisualSender::You,
            Self::Assistant | Self::Streaming | Self::Tool => VisualSender::Assistant,
            Self::System => VisualSender::System,
        }
    }
}

/// Partition rows into visual chat groups (same speaker stays together).
pub fn group_timeline_rows(rows: &[TimelineRow]) -> Vec<&[TimelineRow]> {
    let mut groups = Vec::new();
    let mut start = 0;
    while start < rows.len() {
        let sender = rows[start].kind.visual_sender();
        let mut end = start + 1;
        while end < rows.len() && rows[end].kind.visual_sender() == sender {
            end += 1;
        }
        groups.push(&rows[start..end]);
        start = end;
    }
    groups
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCardStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimelineRow {
    pub id: String,
    pub kind: TimelineRowKind,
    pub label: String,
    pub body: String,
    pub streaming: bool,
    /// Assistant / streaming prose uses Markdown when the body needs it.
    pub render_markdown: bool,
    pub tool_status: Option<ToolCardStatus>,
    /// Full detail for expandable tool rows (args / result).
    pub detail: Option<String>,
    /// Thinking payload (committed or live stream text). Always shown muted.
    pub thinking: Option<String>,
    /// Live thinking in progress (may accompany [`Self::thinking`] text).
    pub thinking_live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct TimelineViewModel {
    pub rows: Vec<TimelineRow>,
    pub selected_agent_id: Option<String>,
    pub selected_agent_name: Option<String>,
}

struct FoldedToolResult {
    is_error: bool,
    preview: String,
    detail: String,
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
    let mut tool_item_ids = HashSet::new();
    let mut results: HashMap<String, FoldedToolResult> = HashMap::new();

    for item in items {
        match item {
            TimelineItem::Tool(tool) => {
                tool_item_ids.insert(tool.tool_call_id.clone());
            }
            TimelineItem::Committed(committed) => {
                if let Message::ToolResult {
                    tool_call_id,
                    content,
                    is_error,
                    details,
                    ..
                } = &committed.message
                {
                    let preview = blocks_text(content);
                    let mut detail = String::new();
                    if let Some(d) = details {
                        detail.push_str(&pretty_json(d));
                    }
                    let text = blocks_text(content);
                    if !text.is_empty() {
                        if !detail.is_empty() {
                            detail.push_str("\n\n");
                        }
                        detail.push_str(&text);
                    }
                    results.insert(
                        tool_call_id.clone(),
                        FoldedToolResult {
                            is_error: is_error == &Some(true),
                            preview,
                            detail,
                        },
                    );
                }
            }
            TimelineItem::RealtimeDraft(_) => {}
        }
    }

    items
        .iter()
        .filter_map(|item| match item {
            TimelineItem::Tool(tool) => Some(row_from_tool(tool)),
            TimelineItem::RealtimeDraft(draft) => Some(row_from_draft(draft)),
            TimelineItem::Committed(committed) => {
                row_from_committed(committed, &tool_item_ids, &results)
            }
        })
        .collect()
}

fn row_from_tool(tool: &ToolItem) -> TimelineRow {
    let status = match tool.status {
        CoreToolStatus::Running => ToolCardStatus::Running,
        CoreToolStatus::Completed => ToolCardStatus::Completed,
        CoreToolStatus::Failed => ToolCardStatus::Failed,
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
    let body = match status {
        ToolCardStatus::Failed => result_preview(tool.result.as_ref()),
        _ => String::new(),
    };
    TimelineRow {
        id: format!("tool-{}", tool.tool_call_id),
        kind: TimelineRowKind::Tool,
        label: tool.tool_name.clone(),
        body,
        streaming: false,
        render_markdown: false,
        tool_status: Some(status),
        detail: Some(detail),
        thinking: None,
        thinking_live: false,
    }
}

fn row_from_committed(
    item: &CommittedItem,
    tool_item_ids: &HashSet<String>,
    results: &HashMap<String, FoldedToolResult>,
) -> Option<TimelineRow> {
    match &item.message {
        Message::ToolResult { .. } => None,
        Message::ToolCall {
            id,
            name,
            arguments,
            ..
        } => {
            if tool_item_ids.contains(id) {
                return None;
            }
            let folded = results.get(id);
            let status = match folded {
                Some(r) if r.is_error => ToolCardStatus::Failed,
                Some(_) => ToolCardStatus::Completed,
                None => ToolCardStatus::Running,
            };
            let mut detail = format!("Arguments\n{}", pretty_json(arguments));
            if let Some(r) = folded
                && !r.detail.is_empty()
            {
                detail.push_str("\n\nResult\n");
                detail.push_str(&r.detail);
            }
            let body = match (status, folded) {
                (ToolCardStatus::Failed, Some(r)) => truncate_preview(&r.preview),
                _ => String::new(),
            };
            Some(TimelineRow {
                id: item.message_id.clone(),
                kind: TimelineRowKind::Tool,
                label: name.clone(),
                body,
                streaming: false,
                render_markdown: false,
                tool_status: Some(status),
                detail: Some(detail),
                thinking: None,
                thinking_live: false,
            })
        }
        Message::User { content, .. } => Some(TimelineRow {
            id: item.message_id.clone(),
            kind: TimelineRowKind::User,
            label: crate::t!("island.timeline.sender.you"),
            body: content_text(content),
            streaming: false,
            render_markdown: false,
            tool_status: None,
            detail: None,
            thinking: None,
            thinking_live: false,
        }),
        Message::Assistant { content, .. } => {
            let (body, thinking) = split_assistant_content(content);
            Some(TimelineRow {
                id: item.message_id.clone(),
                kind: TimelineRowKind::Assistant,
                label: crate::t!("island.timeline.sender.assistant"),
                body,
                streaming: false,
                render_markdown: true,
                tool_status: None,
                detail: None,
                thinking,
                thinking_live: false,
            })
        }
        Message::Context { content, .. } => Some(TimelineRow {
            id: item.message_id.clone(),
            kind: TimelineRowKind::System,
            label: "context".into(),
            body: content_text(content),
            streaming: false,
            render_markdown: false,
            tool_status: None,
            detail: None,
            thinking: None,
            thinking_live: false,
        }),
    }
}

fn row_from_draft(draft: &RealtimeDraft) -> TimelineRow {
    let body = draft.text_segments.join("");
    let thinking_raw = draft.thinking_segments.join("");
    let thinking_live = !thinking_raw.is_empty();
    let thinking = if thinking_raw.is_empty() {
        None
    } else {
        Some(thinking_raw)
    };
    TimelineRow {
        id: draft.message_id.clone(),
        kind: TimelineRowKind::Streaming,
        label: crate::t!("island.timeline.sender.assistant"),
        body,
        streaming: true,
        render_markdown: true,
        tool_status: None,
        detail: None,
        thinking,
        thinking_live,
    }
}

fn split_assistant_content(blocks: &[ContentBlock]) -> (String, Option<String>) {
    let mut text_parts = Vec::new();
    let mut thinking_parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text { text } => text_parts.push(text.clone()),
            ContentBlock::Thinking { thinking, .. } => thinking_parts.push(thinking.clone()),
            ContentBlock::Image { mime_type, .. } => {
                text_parts.push(format!("*[image:{mime_type}]*"));
            }
        }
    }
    let thinking = if thinking_parts.is_empty() {
        None
    } else {
        Some(thinking_parts.join("\n\n"))
    };
    (text_parts.join("\n\n"), thinking)
}

fn result_preview(result: Option<&serde_json::Value>) -> String {
    let Some(value) = result else {
        return String::new();
    };
    match value {
        serde_json::Value::String(s) => truncate_preview(s),
        other => truncate_preview(&other.to_string()),
    }
}

fn truncate_preview(text: &str) -> String {
    const MAX: usize = 120;
    let one_line = text.lines().next().unwrap_or(text).trim();
    if one_line.chars().count() <= MAX {
        return one_line.to_string();
    }
    let truncated: String = one_line.chars().take(MAX.saturating_sub(1)).collect();
    format!("{truncated}…")
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
