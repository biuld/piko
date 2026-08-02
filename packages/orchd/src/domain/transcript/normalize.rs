// ---- Domain: transcript model-view normalization ----
//
// A deterministic projection of the committed transcript used for model
// requests (F-04 / D-04). Tool-result text above the configured cap is
// truncated to its head with an explicit model-visible marker; non-text
// blocks and all metadata are preserved. The committed transcript is never
// mutated — truncation exists only in the model view.

use piko_protocol::messages::{ContentBlock, Message};

use super::snapshot::TranscriptSnapshot;
use super::tokens;

/// Default cap for a single tool result in the model view (≈72 KB of text
/// at the documented estimator). Settings wiring is a follow-on slice.
pub const DEFAULT_MAX_TOOL_OUTPUT_TOKENS: u64 = 24_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptPolicy {
    pub max_tool_output_tokens: u64,
}

impl Default for TranscriptPolicy {
    fn default() -> Self {
        Self {
            max_tool_output_tokens: DEFAULT_MAX_TOOL_OUTPUT_TOKENS,
        }
    }
}

/// A normalized model view: the projected messages with fresh estimates.
pub struct NormalizedTranscript {
    pub snapshot: TranscriptSnapshot,
    /// Number of tool results truncated in this projection.
    pub truncated_outputs: usize,
}

impl NormalizedTranscript {
    pub fn new(messages: Vec<Message>, truncated_outputs: usize) -> Self {
        let tokens = tokens::estimate_messages(&messages);
        Self {
            snapshot: TranscriptSnapshot::new(messages, tokens),
            truncated_outputs,
        }
    }
}

pub fn normalize(messages: &[Message], policy: &TranscriptPolicy) -> (Vec<Message>, usize) {
    let mut truncated = 0;
    let normalized = messages
        .iter()
        .map(|message| normalize_message(message, policy, &mut truncated))
        .collect();
    (normalized, truncated)
}

fn normalize_message(
    message: &Message,
    policy: &TranscriptPolicy,
    truncated: &mut usize,
) -> Message {
    let Message::ToolResult {
        tool_call_id,
        tool_name,
        content,
        details,
        is_error,
        timestamp,
    } = message
    else {
        return message.clone();
    };

    let text_tokens: u64 = content
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => tokens::text_tokens(text),
            _ => 0,
        })
        .sum();
    if text_tokens <= policy.max_tool_output_tokens {
        return message.clone();
    }
    *truncated += 1;

    let total_chars: usize = content
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => text.chars().count(),
            _ => 0,
        })
        .sum();
    // The estimator charges ceil(bytes/3) per text token, so the byte budget
    // for a token cap is `cap * 3`. Text is cut on character boundaries, so
    // multi-byte UTF-8 never splits mid-codepoint.
    let budget_bytes = (policy.max_tool_output_tokens * 3) as usize;

    let mut new_blocks = Vec::with_capacity(content.len() + 1);
    let mut remaining = budget_bytes;
    let mut kept_chars = 0usize;
    let mut needs_marker = false;

    for block in content {
        match block {
            ContentBlock::Text { text } => {
                if remaining == 0 {
                    // Budget already consumed by an earlier text block.
                    needs_marker = true;
                    continue;
                }
                if text.len() <= remaining {
                    kept_chars += text.chars().count();
                    remaining -= text.len();
                    new_blocks.push(ContentBlock::Text { text: text.clone() });
                } else {
                    let (head, kept) = truncate_head(text, remaining);
                    kept_chars += kept;
                    remaining = 0;
                    new_blocks.push(ContentBlock::Text { text: head });
                    needs_marker = true;
                }
            }
            // Images and thinking blocks are preserved and charged in full.
            other => new_blocks.push(other.clone()),
        }
    }
    if needs_marker {
        new_blocks.push(ContentBlock::Text {
            text: truncation_marker(kept_chars, total_chars),
        });
    }

    Message::ToolResult {
        tool_call_id: tool_call_id.clone(),
        tool_name: tool_name.clone(),
        content: new_blocks,
        details: details.clone(),
        is_error: *is_error,
        timestamp: *timestamp,
    }
}

fn truncation_marker(kept: usize, total: usize) -> String {
    format!(
        "[Tool output truncated: retained {kept} of {total} characters. The full output is preserved in session history — read the file or re-run the tool to inspect the remainder.]"
    )
}

fn truncate_head(text: &str, budget_bytes: usize) -> (String, usize) {
    // Keep at least one character so the truncated block is never empty.
    let first = text
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    if budget_bytes == 0 {
        return (first, 1);
    }
    let mut used = 0usize;
    let mut kept = 0usize;
    for (index, ch) in text.char_indices() {
        let end = index + ch.len_utf8();
        if end > budget_bytes {
            break;
        }
        used = end;
        kept += 1;
    }
    if kept == 0 {
        (first, 1)
    } else {
        (text[..used].to_string(), kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::messages::MessageContent;

    fn tool_result(text: &str, details: Option<serde_json::Value>) -> Message {
        Message::ToolResult {
            tool_call_id: "call-1".into(),
            tool_name: Some("bash".into()),
            content: vec![ContentBlock::Text { text: text.into() }],
            details,
            is_error: Some(false),
            timestamp: None,
        }
    }

    fn tool_text(message: &Message) -> String {
        let Message::ToolResult { content, .. } = message else {
            panic!("expected tool result");
        };
        content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn small_output_passes_through_unchanged() {
        let message = tool_result(
            "hello world",
            Some(serde_json::json!({ "full": "hello world" })),
        );
        let (normalized, truncated) =
            normalize(std::slice::from_ref(&message), &TranscriptPolicy::default());
        assert_eq!(truncated, 0);
        assert_eq!(normalized, vec![message]);
    }

    #[test]
    fn oversized_output_is_truncated_with_marker() {
        let huge = "x".repeat(100_000);
        let message = tool_result(&huge, Some(serde_json::json!({ "full": huge })));
        let (normalized, truncated) =
            normalize(std::slice::from_ref(&message), &TranscriptPolicy::default());
        assert_eq!(truncated, 1);
        let text = tool_text(&normalized[0]);
        assert!(text.contains("Tool output truncated: retained"));
        assert!(text.contains("of 100000 characters"));
        assert!(text.contains("read the file or re-run the tool"));
        // The full record stays in the committed message's metadata.
        let Message::ToolResult {
            details,
            is_error,
            tool_name,
            tool_call_id,
            ..
        } = &normalized[0]
        else {
            unreachable!()
        };
        assert_eq!(details.as_ref().unwrap()["full"], huge);
        assert_eq!(is_error, &Some(false));
        assert_eq!(tool_name.as_deref(), Some("bash"));
        assert_eq!(tool_call_id, "call-1");
    }

    #[test]
    fn images_are_preserved_when_text_is_truncated() {
        let message = Message::ToolResult {
            tool_call_id: "call-img".into(),
            tool_name: Some("view_image".into()),
            content: vec![
                ContentBlock::Image {
                    data: "base64-payload".into(),
                    mime_type: "image/png".into(),
                },
                ContentBlock::Text {
                    text: "z".repeat(90_000),
                },
            ],
            details: None,
            is_error: Some(false),
            timestamp: None,
        };
        let (normalized, truncated) = normalize(&[message], &TranscriptPolicy::default());
        assert_eq!(truncated, 1);
        let Message::ToolResult { content, .. } = &normalized[0] else {
            unreachable!()
        };
        assert!(content
            .iter()
            .any(|block| matches!(block, ContentBlock::Image { mime_type, .. } if mime_type == "image/png")));
        assert!(content.iter().any(|block| matches!(
            block,
            ContentBlock::Text { text } if text.contains("Tool output truncated")
        )));
    }

    #[test]
    fn multi_block_budget_is_consumed_in_order() {
        let message = Message::ToolResult {
            tool_call_id: "call-multi".into(),
            tool_name: Some("bash".into()),
            content: vec![
                ContentBlock::Text {
                    text: "first block".into(),
                },
                ContentBlock::Text {
                    text: "y".repeat(100_000),
                },
            ],
            details: None,
            is_error: Some(false),
            timestamp: None,
        };
        let (normalized, truncated) = normalize(&[message], &TranscriptPolicy::default());
        assert_eq!(truncated, 1);
        let text = tool_text(&normalized[0]);
        assert!(text.starts_with("first block"));
        assert!(text.contains("Tool output truncated"));
        assert!(text.contains("of 100011 characters"));
    }

    #[test]
    fn normalization_is_deterministic() {
        let huge = "q".repeat(80_000);
        let message = tool_result(&huge, None);
        let first = normalize(std::slice::from_ref(&message), &TranscriptPolicy::default());
        let second = normalize(&[message], &TranscriptPolicy::default());
        assert_eq!(first, second);
    }

    #[test]
    fn normalizing_does_not_touch_user_messages() {
        let user = Message::User {
            content: MessageContent::String("y".repeat(200_000)),
            timestamp: None,
        };
        let (normalized, truncated) =
            normalize(std::slice::from_ref(&user), &TranscriptPolicy::default());
        assert_eq!(truncated, 0);
        assert_eq!(normalized, vec![user]);
    }
}
