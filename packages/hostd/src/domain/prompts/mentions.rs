//! Parse and resolve user file/skill mentions (F-03 / D-27).

use std::fs;
use std::path::{Path, PathBuf};

use piko_protocol::{
    FileMentionBody, Message, SkillMentionBody, file_mention_context_message,
    skill_mention_context_message,
};

use super::skills::Skill;

/// Ordered mention tokens extracted from user text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MentionToken {
    File { path: String },
    Skill { name: String },
}

/// Parse `@path` and `$skill` mentions in appearance order (deduped).
pub fn parse_mentions(text: &str) -> Vec<MentionToken> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut seen_files = std::collections::HashSet::new();
    let mut seen_skills = std::collections::HashSet::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'@'
            && is_token_start(bytes, index)
            && let Some((path, end)) = parse_file_path(text, bytes, index + 1)
        {
            if seen_files.insert(path.to_string()) {
                out.push(MentionToken::File {
                    path: path.to_string(),
                });
            }
            index = end;
            continue;
        }
        if byte == b'$'
            && is_token_start(bytes, index)
            && let Some((name, end)) = parse_skill_name(text, bytes, index + 1)
        {
            if !is_common_env_var(name) && seen_skills.insert(name.to_string()) {
                out.push(MentionToken::Skill {
                    name: name.to_string(),
                });
            }
            index = end;
            continue;
        }
        index += 1;
    }
    out
}

/// Resolve mentions into retained Context messages (cwd + skill catalog).
pub fn resolve_mention_messages(
    tokens: &[MentionToken],
    cwd: &Path,
    skills: &[Skill],
) -> Vec<Message> {
    tokens
        .iter()
        .map(|token| match token {
            MentionToken::File { path } => resolve_file_mention(path, cwd),
            MentionToken::Skill { name } => resolve_skill_mention(name, skills),
        })
        .collect()
}

fn resolve_file_mention(raw_path: &str, cwd: &Path) -> Message {
    match read_workspace_file(raw_path, cwd) {
        Ok((display, body)) => file_mention_context_message(&display, FileMentionBody::Ok(body)),
        Err(error) => file_mention_context_message(raw_path, FileMentionBody::Err(error)),
    }
}

fn resolve_skill_mention(name: &str, skills: &[Skill]) -> Message {
    let Some(skill) = skills.iter().find(|s| s.name == name) else {
        return skill_mention_context_message(name, SkillMentionBody::Err("unknown skill"));
    };
    match fs::read_to_string(&skill.file_path) {
        Ok(body) => {
            let location = skill.file_path.to_string_lossy().replace('\\', "/");
            skill_mention_context_message(
                name,
                SkillMentionBody::Ok {
                    location: &location,
                    body: &body,
                },
            )
        }
        Err(_) => {
            skill_mention_context_message(name, SkillMentionBody::Err("unreadable skill file"))
        }
    }
}

fn read_workspace_file(raw_path: &str, cwd: &Path) -> Result<(String, String), String> {
    let candidate = if Path::new(raw_path).is_absolute() {
        PathBuf::from(raw_path)
    } else {
        cwd.join(raw_path)
    };
    let cwd_canon = cwd
        .canonicalize()
        .map_err(|_| "workspace unavailable".to_string())?;
    let path_canon = candidate
        .canonicalize()
        .map_err(|_| "path not found".to_string())?;
    if !path_canon.starts_with(&cwd_canon) {
        return Err("path outside workspace".into());
    }
    if !path_canon.is_file() {
        return Err("not a file".into());
    }
    let bytes = fs::read(&path_canon).map_err(|_| "unreadable file".to_string())?;
    if bytes.contains(&0) {
        return Err("binary file".into());
    }
    let body = String::from_utf8(bytes).map_err(|_| "not valid UTF-8".to_string())?;
    let display = path_canon
        .strip_prefix(&cwd_canon)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path_canon.to_string_lossy().replace('\\', "/"));
    Ok((display, body))
}

fn is_token_start(bytes: &[u8], index: usize) -> bool {
    if index == 0 {
        return true;
    }
    let prev = bytes[index - 1];
    !prev.is_ascii_alphanumeric() && prev != b'_'
}

fn parse_file_path<'a>(text: &'a str, bytes: &[u8], start: usize) -> Option<(&'a str, usize)> {
    if start >= bytes.len() {
        return None;
    }
    // First char must look like a path segment, not bare punctuation.
    let first = bytes[start];
    if !(first.is_ascii_alphanumeric()
        || first == b'.'
        || first == b'_'
        || first == b'/'
        || first == b'\\')
    {
        return None;
    }
    let mut end = start + 1;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_whitespace() {
            break;
        }
        // Stop before common trailing sentence punctuation when not path-like.
        if matches!(b, b',' | b';' | b'!' | b'?' | b')' | b']' | b'}') {
            break;
        }
        end += 1;
    }
    let path = text.get(start..end)?;
    if path.is_empty() {
        return None;
    }
    Some((path, end))
}

fn parse_skill_name<'a>(text: &'a str, bytes: &[u8], start: usize) -> Option<(&'a str, usize)> {
    if start >= bytes.len() || !is_skill_name_char(bytes[start]) {
        return None;
    }
    let mut end = start + 1;
    while end < bytes.len() && is_skill_name_char(bytes[end]) {
        end += 1;
    }
    let name = text.get(start..end)?;
    if name.is_empty() {
        return None;
    }
    Some((name, end))
}

fn is_skill_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/')
}

fn is_common_env_var(name: &str) -> bool {
    matches!(
        name,
        "PATH"
            | "HOME"
            | "USER"
            | "SHELL"
            | "PWD"
            | "TMPDIR"
            | "TEMP"
            | "TMP"
            | "LANG"
            | "LC_ALL"
            | "TERM"
            | "EDITOR"
            | "VISUAL"
            | "SSH_AUTH_SOCK"
            | "XDG_CONFIG_HOME"
            | "XDG_DATA_HOME"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_file_and_skill_mentions_in_order() {
        let tokens = parse_mentions("see @src/a.rs and $my-skill then @src/a.rs again");
        assert_eq!(
            tokens,
            vec![
                MentionToken::File {
                    path: "src/a.rs".into()
                },
                MentionToken::Skill {
                    name: "my-skill".into()
                },
            ]
        );
    }

    #[test]
    fn skips_email_like_and_env_vars() {
        let tokens = parse_mentions("mail user@example.com and $PATH $ok-skill");
        assert_eq!(
            tokens,
            vec![MentionToken::Skill {
                name: "ok-skill".into()
            }]
        );
    }

    #[test]
    fn resolves_file_under_cwd() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        fs::write(&file, "hello body").unwrap();
        let messages = resolve_mention_messages(
            &[MentionToken::File {
                path: "hello.txt".into(),
            }],
            dir.path(),
            &[],
        );
        assert_eq!(messages.len(), 1);
        match &messages[0] {
            Message::Context {
                content: piko_protocol::MessageContent::String(text),
                source,
                ..
            } => {
                assert_eq!(source.kind, "user.file-mention");
                assert!(text.contains("hello body"));
                assert!(text.contains("path: hello.txt"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn refuses_path_outside_cwd() {
        let dir = tempdir().unwrap();
        let messages = resolve_mention_messages(
            &[MentionToken::File {
                path: "/etc/hosts".into(),
            }],
            dir.path(),
            &[],
        );
        match &messages[0] {
            Message::Context {
                content: piko_protocol::MessageContent::String(text),
                ..
            } => {
                assert!(text.contains("error:"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
