use piko_protocol::{ContentBlock, MessageContent};

use crate::api::ProtocolError;
use crate::domain::prompts::{PromptTemplate, expand_prompt_template};

const MAX_ENCODED_IMAGE_BYTES: usize = 32 * 1024 * 1024;

pub(crate) fn validate_user_content(content: &MessageContent) -> Result<(), ProtocolError> {
    match content {
        MessageContent::String(text) if text.trim().is_empty() => Err(invalid("message is empty")),
        MessageContent::String(_) => Ok(()),
        MessageContent::Blocks(blocks) => {
            let mut meaningful = false;
            let mut image_bytes = 0usize;
            for block in blocks {
                match block {
                    ContentBlock::Text { text } => meaningful |= !text.trim().is_empty(),
                    ContentBlock::Image { data, mime_type } => {
                        if data.is_empty() {
                            return Err(invalid("image data is empty"));
                        }
                        if !matches!(
                            mime_type.as_str(),
                            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
                        ) {
                            return Err(invalid(format!(
                                "unsupported image MIME type: {mime_type}"
                            )));
                        }
                        image_bytes = image_bytes.saturating_add(data.len());
                        meaningful = true;
                    }
                    _ => {
                        return Err(invalid(
                            "user messages may contain only text and image blocks",
                        ));
                    }
                }
            }
            if image_bytes > MAX_ENCODED_IMAGE_BYTES {
                return Err(invalid("encoded image content exceeds 32 MiB"));
            }
            if meaningful {
                Ok(())
            } else {
                Err(invalid("message is empty"))
            }
        }
    }
}

pub(crate) fn text_projection(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => text.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .map(ContentBlock::text_projection)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

pub(super) fn plain_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => text.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

pub(super) fn expand_templates(
    content: MessageContent,
    templates: &[PromptTemplate],
) -> MessageContent {
    match content {
        MessageContent::String(text) => {
            MessageContent::String(expand_prompt_template(&text, templates))
        }
        MessageContent::Blocks(blocks) => MessageContent::Blocks(
            blocks
                .into_iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => ContentBlock::Text {
                        text: expand_prompt_template(&text, templates),
                    },
                    other => other,
                })
                .collect(),
        ),
    }
}

fn invalid(message: impl Into<String>) -> ProtocolError {
    ProtocolError::InvalidCommand(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_projects_image_only_content() {
        let content = MessageContent::Blocks(vec![ContentBlock::Image {
            data: "AA==".into(),
            mime_type: "image/png".into(),
        }]);
        validate_user_content(&content).unwrap();
        assert_eq!(text_projection(&content), "[image: image/png]");
        assert_eq!(plain_text(&content), "");
    }

    #[test]
    fn rejects_non_user_blocks() {
        let content = MessageContent::Blocks(vec![ContentBlock::Thinking {
            thinking: "hidden".into(),
            thinking_signature: None,
        }]);
        assert!(validate_user_content(&content).is_err());
    }
}
