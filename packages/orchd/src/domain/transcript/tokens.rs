// ---- Domain: transcript token estimator ----
//
// One documented conservative estimator shared by transcript accounting and
// the fail-closed budget preflight (F-04 / D-04). The estimator is
// deliberately provider-agnostic and conservative: text costs
// ceil(bytes / 3) tokens, JSON is charged at serialized byte cost, images
// cost encoded bytes plus framing, and every message pays a small framing
// surcharge. Real usage (F-15) may differ and is recorded separately.

use serde::Serialize;

use piko_protocol::messages::{ContentBlock, Message, MessageContent};

pub fn message_tokens(message: &Message) -> u64 {
    let content = match message {
        Message::Context { content, .. } | Message::User { content, .. } => {
            message_content_tokens(content)
        }
        Message::Assistant { content, .. } | Message::ToolResult { content, .. } => {
            blocks_tokens(content)
        }
        Message::ToolCall {
            name, arguments, ..
        } => text_tokens(name).saturating_add(serialized_tokens(arguments)),
    };
    content.saturating_add(16)
}

pub fn message_content_tokens(content: &MessageContent) -> u64 {
    match content {
        MessageContent::String(text) => text_tokens(text),
        MessageContent::Blocks(blocks) => blocks_tokens(blocks),
    }
}

pub fn blocks_tokens(blocks: &[ContentBlock]) -> u64 {
    blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => text_tokens(text),
            ContentBlock::Thinking {
                thinking,
                thinking_signature,
            } => text_tokens(thinking)
                .saturating_add(thinking_signature.as_deref().map(text_tokens).unwrap_or(0)),
            // Base64 tokenization varies. One token per encoded byte plus
            // framing is deliberately conservative across providers.
            ContentBlock::Image { data, mime_type } => data
                .len()
                .saturating_add(mime_type.len())
                .saturating_add(512) as u64,
        })
        .sum()
}

pub fn text_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(3)
}

pub fn serialized_tokens<T: Serialize + ?Sized>(value: &T) -> u64 {
    serde_json::to_vec(value)
        .map(|bytes| (bytes.len() as u64).div_ceil(3))
        .unwrap_or(u64::MAX)
}

/// Estimate every message in order; length matches `messages`.
pub fn estimate_messages(messages: &[Message]) -> Vec<u64> {
    messages.iter().map(message_tokens).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_message(text: &str) -> Message {
        Message::User {
            content: MessageContent::String(text.into()),
            timestamp: None,
        }
    }

    #[test]
    fn text_tokens_rounds_bytes_up() {
        assert_eq!(text_tokens(""), 0);
        assert_eq!(text_tokens("abc"), 1);
        assert_eq!(text_tokens("abcd"), 2);
        // Multi-byte UTF-8 is charged by byte length.
        assert_eq!(text_tokens("你"), 1);
    }

    #[test]
    fn message_tokens_include_framing_surcharge() {
        assert_eq!(message_tokens(&text_message("abc")), 1 + 16);
    }

    #[test]
    fn estimate_messages_matches_sum_of_parts() {
        let messages = vec![
            text_message("hello"),
            Message::ToolCall {
                id: "call-1".into(),
                name: "read".into(),
                arguments: serde_json::json!({ "path": "Cargo.toml" }),
                model: None,
                provider: None,
                timestamp: None,
            },
            Message::ToolResult {
                tool_call_id: "call-1".into(),
                tool_name: Some("read".into()),
                content: vec![ContentBlock::Text {
                    text: "file body".into(),
                }],
                details: None,
                is_error: Some(false),
                timestamp: None,
            },
        ];
        let estimates = estimate_messages(&messages);
        assert_eq!(estimates.len(), messages.len());
        let direct: u64 = messages.iter().map(message_tokens).sum();
        assert_eq!(estimates.iter().sum::<u64>(), direct);
    }
}
