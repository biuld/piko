use piko_protocol::SessionTreeEntry;

pub fn session_entry_label(entry: &SessionTreeEntry) -> String {
    match entry {
        SessionTreeEntry::Message(message) => format!("[{}]", message.message.role()),
        SessionTreeEntry::ThinkingLevelChange(entry) => {
            format!("[thinking: {}]", entry.thinking_level)
        }
        SessionTreeEntry::ModelChange(entry) => {
            format!("[model: {}/{}]", entry.provider, entry.model_id)
        }
        SessionTreeEntry::ActiveToolsChange(entry) => {
            format!("[tools: {}]", entry.active_tool_names.join(", "))
        }
        SessionTreeEntry::Compaction(entry) => format!("[compact: {} tk]", entry.tokens_before),
        SessionTreeEntry::BranchSummary(_) => "[branch summary]".to_string(),
        SessionTreeEntry::Custom(entry) => format!("[custom: {}]", entry.custom_type),
        SessionTreeEntry::CustomMessage(entry) => {
            format!("[custom message: {}]", entry.custom_type)
        }
        SessionTreeEntry::Label(entry) => {
            format!("[label: {}]", entry.label.as_deref().unwrap_or("unlabeled"))
        }
        SessionTreeEntry::SessionInfo(entry) => format!(
            "[session info: {}]",
            entry.name.as_deref().unwrap_or("unnamed")
        ),
        SessionTreeEntry::Leaf(entry) => {
            format!("[leaf -> {}]", entry.target_id.as_deref().unwrap_or("none"))
        }
    }
}

pub fn session_entry_timeline_text(entry: &SessionTreeEntry) -> Option<String> {
    Some(match entry {
        SessionTreeEntry::Message(m) => crate::text::message_to_text(&m.message),
        SessionTreeEntry::ThinkingLevelChange(e) => format!("changed to {}", e.thinking_level),
        SessionTreeEntry::ModelChange(e) => format!("changed to {}/{}", e.provider, e.model_id),
        SessionTreeEntry::ActiveToolsChange(e) => e.active_tool_names.join(", "),
        SessionTreeEntry::Compaction(e) => e.summary.clone(),
        SessionTreeEntry::BranchSummary(e) => e.summary.clone(),
        SessionTreeEntry::Custom(_) => String::new(),
        SessionTreeEntry::CustomMessage(e) => match &e.content {
            piko_protocol::CustomMessageContent::String(s) => s.clone(),
            piko_protocol::CustomMessageContent::Blocks(_) => String::new(),
        },
        SessionTreeEntry::Label(e) => e.label.as_deref().unwrap_or("").to_string(),
        SessionTreeEntry::SessionInfo(e) => e.name.as_deref().unwrap_or("").to_string(),
        SessionTreeEntry::Leaf(e) => e.target_id.as_deref().unwrap_or("none").to_string(),
    })
}

pub fn session_entry_preview_text(entry: &SessionTreeEntry) -> String {
    let text = session_entry_timeline_text(entry).unwrap_or_default();
    compact_single_line(&text, 160)
}

fn compact_single_line(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let mut preview = normalized
        .chars()
        .take(max_chars)
        .collect::<String>()
        .trim_end()
        .to_string();
    preview.push('…');
    preview
}

#[cfg(test)]
mod tests {
    use super::compact_single_line;

    #[test]
    fn compact_single_line_flattens_multiline_unicode_preview() {
        let preview = compact_single_line("第一行\n第二行\t第三行 的内容", 8);
        assert_eq!(preview, "第一行 第二行…");
    }
}
