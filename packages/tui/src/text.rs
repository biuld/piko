use piko_protocol::{ContentBlock, Message, MessageContent};

pub fn message_to_text(message: &Message) -> String {
    match message {
        Message::User { content, .. } => message_content_to_text(content),
        Message::Assistant { content, .. } | Message::ToolResult { content, .. } => content
            .iter()
            .filter_map(content_block_to_text)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

pub fn compact_json(value: &serde_json::Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "<json>".to_string());
    if text.len() > 240 {
        format!("{}...", &text[..240])
    } else {
        text
    }
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
        ContentBlock::ToolCall {
            id,
            name,
            arguments,
            ..
        } => Some(format!("{name}({id}) {}", compact_json(arguments))),
        ContentBlock::Image { mime_type, .. } => Some(format!("[image {mime_type}]")),
    }
}
