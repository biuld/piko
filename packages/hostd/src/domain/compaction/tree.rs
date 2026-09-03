// ---- Domain: compaction tree projection — active branch, context rewrite, cut points ----

use crate::api::{ContentBlock, Message, MessageContent, SessionTreeEntry};

pub fn active_branch_entries(
    entries: &[SessionTreeEntry],
    leaf_id: Option<&str>,
) -> Vec<SessionTreeEntry> {
    let Some(leaf_id) = leaf_id else {
        return Vec::new();
    };
    let mut by_id = std::collections::HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        by_id.insert(entry.id(), index);
    }

    let mut current = Some(leaf_id.to_string());
    let mut indexes = Vec::new();
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = current {
        // Corrupt or legacy projections can contain parent cycles (e.g. a root
        // message grafted under the final head). Never loop forever.
        if !visited.insert(id.clone()) {
            break;
        }
        let Some(index) = by_id.get(id.as_str()).copied() else {
            break;
        };
        let entry = &entries[index];
        indexes.push(index);
        current = entry.parent_id().map(str::to_string);
    }
    indexes.sort_unstable();
    indexes.dedup();
    indexes
        .into_iter()
        .map(|index| entries[index].clone())
        .collect()
}

pub fn context_entries_after_compaction(entries: &[SessionTreeEntry]) -> Vec<SessionTreeEntry> {
    let Some((compaction_index, compaction)) =
        entries
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, entry)| match entry {
                SessionTreeEntry::Compaction(compaction) => Some((index, compaction)),
                _ => None,
            })
    else {
        return entries.to_vec();
    };

    let first_kept_index = entries
        .iter()
        .position(|entry| entry.id() == compaction.first_kept_entry_id)
        .unwrap_or(compaction_index + 1);

    std::iter::once(entries[compaction_index].clone())
        .chain(entries[first_kept_index..compaction_index].iter().cloned())
        .chain(entries.iter().skip(compaction_index + 1).cloned())
        .collect()
}

pub struct CutPointResult {
    pub first_kept_entry_index: usize,
}

pub fn find_valid_cut_points(
    entries: &[SessionTreeEntry],
    start_index: usize,
    end_index: usize,
) -> Vec<usize> {
    let mut cut_points = Vec::new();
    for (i, entry) in entries
        .iter()
        .enumerate()
        .skip(start_index)
        .take(end_index - start_index)
    {
        if is_valid_cut_point(entry) {
            cut_points.push(i);
        }
    }
    cut_points
}

pub fn find_cut_point(
    entries: &[SessionTreeEntry],
    start_index: usize,
    end_index: usize,
    keep_recent_tokens: u64,
    estimate_entry_tokens: impl Fn(&SessionTreeEntry) -> u64,
) -> CutPointResult {
    let cut_points = find_valid_cut_points(entries, start_index, end_index);
    if cut_points.is_empty() {
        return CutPointResult {
            first_kept_entry_index: start_index,
        };
    }

    let mut accumulated_tokens = 0;
    let mut cut_index = cut_points[0];

    for i in (start_index..end_index).rev() {
        let tokens = estimate_entry_tokens(&entries[i]);
        accumulated_tokens += tokens;

        if accumulated_tokens >= keep_recent_tokens {
            for &cp in &cut_points {
                if cp >= i {
                    cut_index = cp;
                    break;
                }
            }
            break;
        }
    }

    CutPointResult {
        first_kept_entry_index: cut_index,
    }
}

pub fn entry_role(entry: &SessionTreeEntry) -> Option<&str> {
    match entry {
        SessionTreeEntry::Message(message_entry) => Some(message_entry.message.role()),
        SessionTreeEntry::Compaction(_) => Some("compactionSummary"),
        _ => None,
    }
}

pub fn entry_text(entry: &SessionTreeEntry) -> String {
    match entry {
        SessionTreeEntry::Message(message_entry) => message_text(&message_entry.message),
        SessionTreeEntry::Compaction(compaction) => compaction.summary.clone(),
        SessionTreeEntry::BranchSummary(summary) => summary.summary.clone(),
        SessionTreeEntry::CustomMessage(custom) => match &custom.content {
            crate::api::CustomMessageContent::String(text) => text.clone(),
            crate::api::CustomMessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(content_block_text)
                .collect::<Vec<_>>()
                .join(""),
        },
        _ => String::new(),
    }
}

fn is_valid_cut_point(entry: &SessionTreeEntry) -> bool {
    match entry {
        SessionTreeEntry::Message(message_entry) => {
            matches!(
                message_entry.message,
                Message::User { .. } | Message::Assistant { .. }
            )
        }
        SessionTreeEntry::Compaction(_) => true,
        _ => false,
    }
}

fn message_text(message: &Message) -> String {
    match message {
        Message::Context { content, .. } => message_content_text(content),
        Message::User { content, .. } => message_content_text(content),
        Message::Assistant { content, .. } => content
            .iter()
            .filter_map(assistant_content_block_text)
            .collect::<Vec<_>>()
            .join(""),
        Message::ToolResult { content, .. } => content
            .iter()
            .filter_map(content_block_text)
            .collect::<Vec<_>>()
            .join(""),
        Message::ToolCall {
            id,
            name,
            arguments,
            ..
        } => format!("{name}({id}) {}", compact_value(arguments)),
    }
}

fn compact_value(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn message_content_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => text.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(content_block_text)
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn assistant_content_block_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text { text } => Some(text.clone()),
        ContentBlock::Thinking { thinking, .. } => Some(thinking.clone()),
        ContentBlock::Image { .. } => None,
        ContentBlock::UpstreamToolActivity { .. }
        | ContentBlock::UpstreamToolApproval { .. }
        | ContentBlock::Source { .. }
        | ContentBlock::Citation { .. }
        | ContentBlock::Artifact { .. } => Some(block.text_projection()),
    }
}

fn content_block_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text { text } => Some(text.clone()),
        ContentBlock::Thinking { thinking, .. } => Some(thinking.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::{Message, MessageContent, MessageEntry};

    fn message_entry(id: &str, parent_id: Option<&str>, seq: u64) -> SessionTreeEntry {
        SessionTreeEntry::Message(MessageEntry {
            id: id.into(),
            parent_id: parent_id.map(str::to_string),
            timestamp: seq.to_string(),
            agent_id: "main".into(),
            agent_instance_id: "agent-1".into(),
            root_input_id: "turn-1".into(),
            transcript_seq: seq,
            message: Message::User {
                content: MessageContent::String("hi".into()),
                timestamp: None,
            },
        })
    }

    #[test]
    fn parent_cycle_does_not_loop_forever() {
        // A corrupted projection can graft a root message under the final head,
        // making the leaf walk a cycle (a -> b -> a). The walk must terminate.
        let entries = vec![
            message_entry("root-msg", Some("final-msg"), 1),
            message_entry("final-msg", None, 2),
        ];
        let branch = active_branch_entries(&entries, Some("final-msg"));
        assert!(
            branch.len() <= entries.len(),
            "cycle walk must terminate within the entry count"
        );
    }
}
