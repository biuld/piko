use piko_protocol::{ContentBlock, MessageContent};

use crate::api::ProtocolError;
use crate::domain::prompts::{MentionToken, skills::Skill};
use crate::domain::prompts::{PromptTemplate, expand_prompt_template};
use crate::ports::prompt_materials::PromptMaterialLoader;

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

pub(super) fn resolve_mention_messages(
    tokens: &[MentionToken],
    cwd: &str,
    skills: &[Skill],
    loader: &dyn PromptMaterialLoader,
) -> Vec<piko_protocol::Message> {
    tokens
        .iter()
        .map(|token| match token {
            MentionToken::File { path } => match loader.load_mention_file(path, cwd) {
                Some(file) => piko_protocol::file_mention_context_message(
                    &file.display_path,
                    piko_protocol::FileMentionBody::Ok(file.body),
                ),
                None => piko_protocol::file_mention_context_message(
                    path,
                    piko_protocol::FileMentionBody::Err("path not found".into()),
                ),
            },
            MentionToken::Skill { name } => {
                let Some(skill) = skills.iter().find(|skill| skill.name == *name) else {
                    return piko_protocol::skill_mention_context_message(
                        name,
                        piko_protocol::SkillMentionBody::Err("unknown skill"),
                    );
                };
                let location = skill.file_path.to_string_lossy().replace('\\', "/");
                match loader.load_skill_body(&skill.file_path) {
                    Some(body) => piko_protocol::skill_mention_context_message(
                        name,
                        piko_protocol::SkillMentionBody::Ok {
                            location: &location,
                            body: &body,
                        },
                    ),
                    None => piko_protocol::skill_mention_context_message(
                        name,
                        piko_protocol::SkillMentionBody::Err("unreadable skill file"),
                    ),
                }
            }
        })
        .collect()
}

fn invalid(message: impl Into<String>) -> ProtocolError {
    ProtocolError::InvalidCommand(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakePromptMaterials;

    impl PromptMaterialLoader for FakePromptMaterials {
        fn load_prompt_templates(&self, _cwd: &str) -> Vec<PromptTemplate> {
            Vec::new()
        }

        fn load_context_files(&self, _cwd: &str) -> Vec<crate::domain::prompts::ContextFile> {
            Vec::new()
        }

        fn load_skills(&self, _cwd: &str) -> crate::domain::prompts::skills::LoadSkillsResult {
            Default::default()
        }

        fn load_mention_file(
            &self,
            raw_path: &str,
            _cwd: &str,
        ) -> Option<crate::ports::prompt_materials::MentionFile> {
            (raw_path == "src/main.rs").then(|| crate::ports::prompt_materials::MentionFile {
                display_path: raw_path.into(),
                body: "fn main() {}".into(),
            })
        }

        fn load_skill_body(&self, _file_path: &std::path::Path) -> Option<String> {
            None
        }
    }

    #[test]
    fn validates_and_projects_image_only_content() {
        let content = MessageContent::Blocks(vec![ContentBlock::Image {
            data: "AA==".into(),
            mime_type: "image/png".into(),
        }]);
        validate_user_content(&content).unwrap();
        let MessageContent::Blocks(blocks) = &content else {
            panic!("expected blocks");
        };
        assert_eq!(
            blocks
                .iter()
                .map(ContentBlock::text_projection)
                .collect::<Vec<_>>()
                .join("\n"),
            "[image: image/png]"
        );
        assert_eq!(plain_text(&content), "");
    }

    #[test]
    fn rejects_non_user_blocks() {
        let content = MessageContent::Blocks(vec![ContentBlock::Thinking {
            thinking: "hidden".into(),
            thinking_signature: None,
            duration_ms: None,
        }]);
        assert!(validate_user_content(&content).is_err());
    }

    #[test]
    fn resolves_mentions_at_the_application_port_boundary() {
        let messages = resolve_mention_messages(
            &[
                MentionToken::File {
                    path: "src/main.rs".into(),
                },
                MentionToken::Skill {
                    name: "missing".into(),
                },
            ],
            "/project",
            &[],
            &FakePromptMaterials,
        );

        assert_eq!(messages.len(), 2);
        let rendered = format!("{messages:?}");
        assert!(rendered.contains("fn main() {}"));
        assert!(rendered.contains("unknown skill"));
    }
}
