use serde_json::Value;

/// Max lines of multi-line field bodies in expanded view.
pub(super) const MAX_BODY_LINES: usize = 80;
/// Max characters for a single-line collapsed preview fragment.
pub(super) const PREVIEW_CHARS: usize = 96;

pub(super) fn parse_json(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

pub(super) fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub(super) fn display_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub(super) fn body_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if lines.is_empty() {
        return vec![String::new()];
    }
    if lines.len() > MAX_BODY_LINES {
        let omitted = lines.len() - MAX_BODY_LINES;
        lines.truncate(MAX_BODY_LINES);
        lines.push(format!("... ({omitted} more lines)"));
    }
    lines
}

pub(super) fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn clip(text: String, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text;
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

pub(super) fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

pub(super) fn object_entries(value: &Value) -> Vec<(&str, &Value)> {
    value
        .as_object()
        .map(|map| map.iter().map(|(k, v)| (k.as_str(), v)).collect())
        .unwrap_or_default()
}

pub(super) fn primary_string_preview(value: &Value) -> Option<String> {
    const PREFERRED: &[&str] = &[
        "path", "cmd", "command", "query", "prompt", "message", "question", "title", "content",
        "text", "name", "url", "id",
    ];
    let obj = value.as_object()?;
    for key in PREFERRED {
        if let Some(Value::String(s)) = obj.get(*key)
            && !s.is_empty()
        {
            return Some(clip(single_line(s), PREVIEW_CHARS));
        }
    }
    obj.values().find_map(|v| match v {
        Value::String(s) if !s.is_empty() => Some(clip(single_line(s), PREVIEW_CHARS)),
        _ => None,
    })
}
