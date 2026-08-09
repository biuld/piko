pub(crate) fn sanitize_diagnostic(message: &str) -> String {
    let mut sanitized = message
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    for word in message.split_whitespace() {
        let jwt_like = word.len() > 32 && word.matches('.').count() == 2;
        if word.starts_with("sk-") || jwt_like {
            sanitized = sanitized.replace(word, "[REDACTED]");
        }
    }
    let mut search_from = 0;
    while let Some(offset) = sanitized[search_from..]
        .to_ascii_lowercase()
        .find("bearer ")
    {
        let start = search_from + offset;
        let token_start = start + "bearer ".len();
        let token_end = sanitized[token_start..]
            .find(char::is_whitespace)
            .map(|offset| token_start + offset)
            .unwrap_or(sanitized.len());
        sanitized.replace_range(token_start..token_end, "[REDACTED]");
        search_from = token_start + "[REDACTED]".len();
    }
    sanitized.chars().take(1_024).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_bearer_and_key_shapes() {
        let value = sanitize_diagnostic("Bearer secret-token sk-example");
        assert_eq!(value, "Bearer [REDACTED] [REDACTED]");
    }
}
