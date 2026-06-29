use piko_protocol::SessionTreeEntry;
use ratatui::{Frame, layout::Rect};

use crate::{
    components::filterable_list::{FilterableItem, FilterableList, render_filterable_list},
    panels::short_id,
};

/// A single displayable row in the session tree.
#[derive(Clone)]
pub struct TreeEntry {
    pub id: String,
    pub depth: usize,
    pub label: String,
    pub detail: String,
    pub is_current: bool,
}

/// Session tree panel.
pub struct TreePanel {
    pub list: FilterableList<TreeEntry>,
}

impl TreePanel {
    pub fn new() -> Self {
        Self {
            list: FilterableList::new(Vec::new()),
        }
    }

    pub fn load(&mut self, snapshot_entries: &[SessionTreeEntry], current_leaf_id: Option<&str>) {
        let entries = build_tree_entries(snapshot_entries, current_leaf_id);
        let selected = entries.iter().position(|e| e.is_current).unwrap_or(0);
        self.list = FilterableList {
            items: entries,
            selected,
        };
    }

    pub fn select_next(&mut self, filter: &str) {
        self.list.select_next(filter, |item| {
            item.label.to_lowercase().contains(filter)
                || item.detail.to_lowercase().contains(filter)
        });
    }

    pub fn select_prev(&mut self, filter: &str) {
        self.list.select_prev(filter, |item| {
            item.label.to_lowercase().contains(filter)
                || item.detail.to_lowercase().contains(filter)
        });
    }

    pub fn selected_entry_id(&self, filter: &str) -> Option<String> {
        let filtered = self.list.filtered_indices(filter, |item| {
            item.label.to_lowercase().contains(filter)
                || item.detail.to_lowercase().contains(filter)
        });
        if filtered.is_empty() {
            return None;
        }
        let selected_filtered_idx = filtered
            .iter()
            .position(|&orig_idx| orig_idx == self.list.selected)
            .unwrap_or(0)
            .min(filtered.len().saturating_sub(1));
        filtered
            .get(selected_filtered_idx)
            .and_then(|&orig_idx| self.list.items.get(orig_idx))
            .map(|item| item.id.clone())
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, filter: &str) {
        let items: Vec<FilterableItem> = self
            .list
            .items
            .iter()
            .map(|item| {
                let current_marker = if item.is_current { "*" } else { "" };
                let indent = "  ".repeat(item.depth.min(12));
                FilterableItem {
                    primary: format!("{}{}{}", current_marker, indent, item.label),
                    detail: format!("{}{}", indent, item.detail),
                    is_active: item.is_current,
                }
            })
            .collect();
        render_filterable_list(frame, area, "session tree", &items, self.list.selected, filter);
    }
}

// ── tree building helpers ─────────────────────────────────────────────────────

fn build_tree_entries(
    entries: &[SessionTreeEntry],
    current_leaf_id: Option<&str>,
) -> Vec<TreeEntry> {
    entries
        .iter()
        .map(|entry| {
            let id = entry.id().to_string();
            TreeEntry {
                id: id.clone(),
                depth: entry_depth(entries, entry.parent_id(), 0),
                label: session_entry_label(entry),
                detail: format!("{}  {}", short_id(&id), entry.timestamp()),
                is_current: current_leaf_id == Some(id.as_str())
                    || entry.leaf_target_id() == current_leaf_id,
            }
        })
        .collect()
}

fn entry_depth(entries: &[SessionTreeEntry], parent_id: Option<&str>, depth: usize) -> usize {
    let Some(parent_id) = parent_id else {
        return depth;
    };
    let Some(parent) = entries.iter().find(|e| e.id() == parent_id) else {
        return depth;
    };
    entry_depth(entries, parent.parent_id(), depth + 1)
}

fn session_entry_label(entry: &SessionTreeEntry) -> String {
    match entry {
        SessionTreeEntry::Message(message) => format!("message {}", message.message.role()),
        SessionTreeEntry::ThinkingLevelChange(entry) => {
            format!("thinking {}", entry.thinking_level)
        }
        SessionTreeEntry::ModelChange(entry) => {
            format!("model {}/{}", entry.provider, entry.model_id)
        }
        SessionTreeEntry::ActiveToolsChange(entry) => {
            format!("tools {}", entry.active_tool_names.join(", "))
        }
        SessionTreeEntry::Compaction(entry) => {
            format!("compaction {} tokens", entry.tokens_before)
        }
        SessionTreeEntry::BranchSummary(_) => "branch summary".to_string(),
        SessionTreeEntry::Custom(entry) => format!("custom {}", entry.custom_type),
        SessionTreeEntry::CustomMessage(entry) => format!("custom message {}", entry.custom_type),
        SessionTreeEntry::Label(entry) => {
            format!("label {}", entry.label.as_deref().unwrap_or("unlabeled"))
        }
        SessionTreeEntry::SessionInfo(entry) => {
            format!(
                "session info {}",
                entry.name.as_deref().unwrap_or("unnamed")
            )
        }
        SessionTreeEntry::Leaf(entry) => {
            format!("leaf {}", entry.target_id.as_deref().unwrap_or("none"))
        }
    }
}

/// Text to display in the timeline for non-message session entries.
pub fn session_entry_timeline_text(entry: &SessionTreeEntry) -> Option<String> {
    Some(match entry {
        SessionTreeEntry::Message(_) => return None,
        SessionTreeEntry::ThinkingLevelChange(e) => {
            format!("thinking level changed to {}", e.thinking_level)
        }
        SessionTreeEntry::ModelChange(e) => {
            format!("model changed to {}/{}", e.provider, e.model_id)
        }
        SessionTreeEntry::ActiveToolsChange(e) => {
            format!("active tools: {}", e.active_tool_names.join(", "))
        }
        SessionTreeEntry::Compaction(e) => {
            format!("compacted {} tokens: {}", e.tokens_before, e.summary)
        }
        SessionTreeEntry::BranchSummary(e) => format!("branch summary: {}", e.summary),
        SessionTreeEntry::Custom(e) => format!("custom entry {}", e.custom_type),
        SessionTreeEntry::CustomMessage(e) => format!("custom message {}", e.custom_type),
        SessionTreeEntry::Label(e) => {
            format!("label {}", e.label.as_deref().unwrap_or(""))
        }
        SessionTreeEntry::SessionInfo(e) => {
            format!("session info {}", e.name.as_deref().unwrap_or(""))
        }
        SessionTreeEntry::Leaf(e) => {
            format!("leaf -> {}", e.target_id.as_deref().unwrap_or("none"))
        }
    })
}
