//! Pure parsing for user file/skill mentions (F-03 / D-27).

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
}
