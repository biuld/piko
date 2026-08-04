//! User file/skill mention Context helpers (F-03 / D-27).

use crate::{ContentTrust, Message, MessageContent, PromptSource};

/// Source kind for `@path` file-mention Context messages.
pub const FILE_MENTION_SOURCE_KIND: &str = "user.file-mention";

/// Source kind for `$skill` skill-mention Context messages.
pub const SKILL_MENTION_SOURCE_KIND: &str = "user.skill-mention";

/// Max characters of file/skill body retained for the model (excluding headers).
pub const MENTION_BODY_MAX_CHARS: usize = 64_000;

/// Stable message id for a file-mention Context on a given execution.
pub fn file_mention_message_id(execution_id: &str, index: usize) -> String {
    format!("{execution_id}/file-mention/{index}")
}

/// Stable message id for a skill-mention Context on a given execution.
pub fn skill_mention_message_id(execution_id: &str, index: usize) -> String {
    format!("{execution_id}/skill-mention/{index}")
}

/// Successful file mention body (model-visible).
pub fn file_mention_content(path: &str, body: &str) -> String {
    format!(
        "file mention:\npath: {path}\n---\n{}",
        bound_mention_body(body)
    )
}

/// Failed file mention body (fail-soft).
pub fn file_mention_error_content(path: &str, error: &str) -> String {
    format!("file mention:\npath: {path}\nerror: {error}")
}

/// Successful skill mention body.
pub fn skill_mention_content(name: &str, location: &str, body: &str) -> String {
    format!(
        "skill mention:\nname: {name}\nlocation: {location}\n---\n{}",
        bound_mention_body(body)
    )
}

/// Failed skill mention body.
pub fn skill_mention_error_content(name: &str, error: &str) -> String {
    format!("skill mention:\nname: {name}\nerror: {error}")
}

pub fn file_mention_context_message(path: &str, body_or_error: FileMentionBody) -> Message {
    let content = match &body_or_error {
        FileMentionBody::Ok(body) => file_mention_content(path, body),
        FileMentionBody::Err(error) => file_mention_error_content(path, error),
    };
    Message::Context {
        content: MessageContent::String(content),
        trust: ContentTrust::WorkspaceControlled,
        source: PromptSource::new(FILE_MENTION_SOURCE_KIND, path),
        timestamp: Some(now_ms()),
    }
}

pub fn skill_mention_context_message(name: &str, body_or_error: SkillMentionBody<'_>) -> Message {
    let content = match body_or_error {
        SkillMentionBody::Ok { location, body } => skill_mention_content(name, location, body),
        SkillMentionBody::Err(error) => skill_mention_error_content(name, error),
    };
    Message::Context {
        content: MessageContent::String(content),
        trust: ContentTrust::WorkspaceControlled,
        source: PromptSource::new(SKILL_MENTION_SOURCE_KIND, name),
        timestamp: Some(now_ms()),
    }
}

#[derive(Debug, Clone)]
pub enum FileMentionBody {
    Ok(String),
    Err(String),
}

#[derive(Debug, Clone, Copy)]
pub enum SkillMentionBody<'a> {
    Ok { location: &'a str, body: &'a str },
    Err(&'a str),
}

pub fn bound_mention_body(text: &str) -> String {
    let count = text.chars().count();
    if count <= MENTION_BODY_MAX_CHARS {
        return text.to_string();
    }
    let mut out: String = text
        .chars()
        .take(MENTION_BODY_MAX_CHARS.saturating_sub(1))
        .collect();
    out.push('…');
    out
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_mention_content_is_stable() {
        let text = file_mention_content("src/main.rs", "fn main() {}");
        assert_eq!(text, "file mention:\npath: src/main.rs\n---\nfn main() {}");
    }

    #[test]
    fn body_truncates_with_ellipsis() {
        let big = "x".repeat(MENTION_BODY_MAX_CHARS + 10);
        let bound = bound_mention_body(&big);
        assert_eq!(bound.chars().count(), MENTION_BODY_MAX_CHARS);
        assert!(bound.ends_with('…'));
    }
}
