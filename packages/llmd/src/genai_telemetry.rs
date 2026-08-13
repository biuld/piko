//! OpenTelemetry GenAI semantic-convention projection.
//!
//! Content attributes are opt-in and bounded. This module deliberately maps
//! the provider-neutral request instead of any adapter-private wire payload.

use piko_protocol::{ContentBlock, MessageContent};
use serde_json::{Value, json};

use crate::gateway::{ConversationItemKind, InferenceRequest};

const MAX_ATTRIBUTE_BYTES: usize = 256 * 1024;
const MAX_ITEMS: usize = 128;
const MAX_TEXT_CHARS: usize = 64 * 1024;

pub(crate) fn content_attributes(
    request: &InferenceRequest,
) -> crate::telemetry::GenAiContentAttributes {
    let system = request
        .conversation
        .instructions
        .blocks
        .iter()
        .take(MAX_ITEMS)
        .map(|block| json!({"type":"text","content":bounded_text(&block.content)}))
        .collect::<Vec<_>>();
    let messages = request
        .conversation
        .items
        .iter()
        .take(MAX_ITEMS)
        .map(|item| input_message(&item.kind))
        .collect::<Vec<_>>();
    let tools = request
        .tools
        .iter()
        .take(MAX_ITEMS)
        .map(|tool| {
            if let Some(definition) = tool.caller() {
                json!({
                    "type": "function",
                    "name": definition.name,
                    "description": bounded_text(&definition.description),
                    "parameters": definition.input_schema,
                })
            } else {
                json!({
                    "type": "function",
                    "name": tool.name(),
                    "description": "Upstream provider tool",
                    "parameters": {"type":"object"},
                })
            }
        })
        .collect::<Vec<_>>();

    let source_was_truncated = request.conversation.instructions.blocks.len() > MAX_ITEMS
        || request.conversation.items.len() > MAX_ITEMS
        || request.tools.len() > MAX_ITEMS;
    let (system_instructions, system_dropped) = serialize_bounded(&system);
    let (input_messages, input_dropped) = serialize_bounded(&messages);
    let (tool_definitions, tools_dropped) = serialize_bounded(&tools);
    crate::telemetry::GenAiContentAttributes {
        system_instructions,
        input_messages,
        tool_definitions,
        dropped: source_was_truncated || system_dropped || input_dropped || tools_dropped,
    }
}

fn input_message(item: &ConversationItemKind) -> Value {
    match item {
        ConversationItemKind::Context {
            content,
            trust,
            source,
        } => json!({
            "role":"user",
            "parts":[text_part(&format!(
                "[piko data-only context; authority=None; trust={trust:?}; source={}:{}]\n{}",
                source.kind,
                source.locator,
                content_text(content)
            ))]
        }),
        ConversationItemKind::User { content } => {
            json!({"role":"user","parts":content_parts(content)})
        }
        ConversationItemKind::Assistant { content } => json!({
            "role":"assistant",
            "parts":content.iter().filter_map(content_block_part).collect::<Vec<_>>()
        }),
        ConversationItemKind::ToolCall {
            call_id,
            name,
            arguments,
        } => json!({
            "role":"assistant",
            "parts":[{"type":"tool_call","id":call_id.0,"name":name,"arguments":arguments}]
        }),
        ConversationItemKind::ToolResult {
            call_id, content, ..
        } => json!({
            "role":"tool",
            "parts":[{
                "type":"tool_call_response",
                "id":call_id.0,
                "response":content.iter().filter_map(content_block_part).collect::<Vec<_>>()
            }]
        }),
        other => json!({
            "role":"assistant",
            "parts":[text_part(&format!("{other:?}"))]
        }),
    }
}

fn content_parts(content: &MessageContent) -> Vec<Value> {
    match content {
        MessageContent::String(text) => vec![text_part(text)],
        MessageContent::Blocks(blocks) => blocks.iter().filter_map(content_block_part).collect(),
    }
}

fn content_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => bounded_text(text),
        MessageContent::Blocks(blocks) => bounded_text(
            &blocks
                .iter()
                .filter(|block| !matches!(block, ContentBlock::Thinking { .. }))
                .map(ContentBlock::text_projection)
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    }
}

fn content_block_part(block: &ContentBlock) -> Option<Value> {
    match block {
        ContentBlock::Text { text } => Some(text_part(text)),
        // Never export hidden chain-of-thought, even when general content
        // capture is enabled.
        ContentBlock::Thinking { .. } => None,
        ContentBlock::Image { mime_type, .. } => Some(text_part(&format!("[image: {mime_type}]"))),
        other => Some(text_part(&other.text_projection())),
    }
}

fn text_part(text: &str) -> Value {
    json!({"type":"text","content":bounded_text(text)})
}

fn bounded_text(text: &str) -> String {
    crate::redaction::sanitize_sensitive_text(text)
        .chars()
        .take(MAX_TEXT_CHARS)
        .collect()
}

fn serialize_bounded(value: &impl serde::Serialize) -> (Option<String>, bool) {
    let Ok(serialized) = serde_json::to_string(value) else {
        return (None, true);
    };
    if serialized.len() > MAX_ATTRIBUTE_BYTES {
        (None, true)
    } else {
        (Some(serialized), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_attributes_are_dropped_instead_of_emitting_invalid_json() {
        let value = Value::String("x".repeat(MAX_ATTRIBUTE_BYTES));
        let (serialized, dropped) = serialize_bounded(&value);
        assert!(serialized.is_none());
        assert!(dropped);
    }

    #[test]
    fn thinking_content_is_never_projected() {
        let part = content_block_part(&ContentBlock::Thinking {
            thinking: "private reasoning".into(),
            thinking_signature: None,
        });
        assert!(part.is_none());
    }
}
