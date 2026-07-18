use piko_protocol::{ContentBlock, Message, MessageContent};

pub fn message_to_text(message: &Message) -> String {
    match message {
        Message::Context { content, .. } => message_content_to_text(content),
        Message::User { content, .. } => message_content_to_text(content),
        Message::Assistant { content, .. } => content
            .iter()
            .filter_map(assistant_content_block_to_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Message::ToolResult { content, .. } => content
            .iter()
            .filter_map(content_block_to_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Message::ToolCall {
            id,
            name,
            arguments,
            ..
        } => format!("{name}({id}) {}", compact_json(arguments)),
    }
}

fn assistant_content_block_to_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text { text } => Some(text.clone()),
        ContentBlock::Thinking { thinking, .. } => Some(format!("[thinking] {thinking}")),
        ContentBlock::Image { mime_type, .. } => Some(format!("[image {mime_type}]")),
    }
}

pub fn compact_json(value: &serde_json::Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "<json>".to_string());
    if text.len() <= 240 {
        return text;
    }
    let mut end = 240;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

fn message_content_to_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => text.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(content_block_to_text)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn content_block_to_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text { text } => Some(text.clone()),
        ContentBlock::Thinking { thinking, .. } => Some(format!("[thinking] {thinking}")),
        ContentBlock::Image { mime_type, .. } => Some(format!("[image {mime_type}]")),
    }
}
