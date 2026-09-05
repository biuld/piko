//! Structured, scrollable bodies for persisted messages and tree entries.
use super::paint::{plain, wrapped};
use crate::theme::Theme;
use piko_protocol::{ContentBlock, Message, MessageContent, SessionTreeEntry};
use ratatui::text::Line;

pub(super) fn section(label: &str, body: &str, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![plain(label, theme.accent, width)];
    lines.extend(wrapped(body, theme.text, width));
    lines.push(Line::from(""));
    lines
}

pub(super) fn fields(value: &serde_json::Value, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    match value {
        serde_json::Value::Object(map) => map
            .iter()
            .flat_map(|(key, value)| {
                let label = key.replace('_', " ");
                match value {
                    serde_json::Value::String(text) => section(&label, text, theme, width),
                    value => section(
                        &label,
                        &serde_json::to_string_pretty(value).unwrap_or_default(),
                        theme,
                        width,
                    ),
                }
            })
            .collect(),
        serde_json::Value::String(text) => wrapped(text, theme.text, width),
        value => wrapped(
            &serde_json::to_string_pretty(value).unwrap_or_default(),
            theme.dim,
            width,
        ),
    }
}

pub(super) fn message_content(
    content: &MessageContent,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    match content {
        MessageContent::String(text) => wrapped(text, theme.text, width),
        MessageContent::Blocks(blocks) => block_lines(blocks, theme, width),
    }
}

pub(super) fn block_lines(
    blocks: &[ContentBlock],
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    blocks
        .iter()
        .flat_map(|block| match block {
            ContentBlock::Text { text } => section("Text", text, theme, width),
            ContentBlock::Thinking { thinking, .. } => {
                let mut lines = vec![plain("Thinking", theme.thinking_text, width)];
                lines.extend(wrapped(thinking, theme.thinking_text, width));
                lines.push(Line::from(""));
                lines
            }
            ContentBlock::Image { mime_type, .. } => section(
                "Image",
                &format!("{mime_type} · image content is not rendered in this terminal"),
                theme,
                width,
            ),
            other => section("Content", &other.text_projection(), theme, width),
        })
        .collect()
}

pub(super) fn message_lines(message: &Message, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    match message {
        Message::User { content, .. } | Message::Context { content, .. } => {
            let role = if matches!(message, Message::User { .. }) {
                "user"
            } else {
                "context"
            };
            let mut lines = vec![plain(role, theme.accent_user, width)];
            lines.extend(message_content(content, theme, width));
            lines
        }
        Message::Assistant { content, .. } => {
            let mut lines = vec![plain("assistant", theme.accent_assistant, width)];
            lines.extend(block_lines(content, theme, width));
            lines
        }
        Message::ToolCall {
            id,
            name,
            arguments,
            ..
        } => {
            let mut lines = section("Tool call", name, theme, width);
            lines.push(plain("Arguments", theme.muted, width));
            lines.extend(fields(arguments, theme, width));
            lines.extend(section("Tool call ID", id, theme, width));
            lines
        }
        Message::ToolResult {
            tool_name,
            content,
            is_error,
            details,
            tool_call_id,
            ..
        } => {
            let mut lines = section(
                "Tool result",
                tool_name.as_deref().unwrap_or("tool"),
                theme,
                width,
            );
            if *is_error == Some(true) {
                lines.push(plain("Failed", theme.error, width));
            }
            lines.extend(block_lines(content, theme, width));
            if let Some(details) = details {
                lines.push(plain("Recorded result details", theme.accent, width));
                lines.extend(fields(details, theme, width));
            }
            lines.extend(section("Tool call ID", tool_call_id, theme, width));
            lines
        }
    }
}

pub(super) fn tree_lines(
    entry: &SessionTreeEntry,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    match entry {
        SessionTreeEntry::Message(entry) => {
            let mut lines = message_lines(&entry.message, theme, width);
            lines.extend(section("Message", &entry.id, theme, width));
            lines.extend(section("Agent", &entry.agent_instance_id, theme, width));
            lines.extend(section("Root input", &entry.root_input_id, theme, width));
            if let Some(parent) = &entry.parent_id {
                lines.extend(section("Parent", parent, theme, width));
            }
            lines
        }
        SessionTreeEntry::ToolCall(entry) => {
            let mut lines = section("Tool call", &entry.tool_name, theme, width);
            lines.extend(fields(&entry.arguments, theme, width));
            lines.extend(section("Call ID", &entry.tool_call_id, theme, width));
            lines
        }
        SessionTreeEntry::Compaction(entry) => {
            section("Compaction summary", &entry.summary, theme, width)
        }
        SessionTreeEntry::BranchSummary(entry) => {
            section("Branch summary", &entry.summary, theme, width)
        }
        other => fields(
            &serde_json::to_value(other).unwrap_or_default(),
            theme,
            width,
        ),
    }
}
