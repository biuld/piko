//! Tree view-model from session tree entries.

use std::collections::{HashMap, HashSet};

use piko_client_core::{ClientState, active_path_ids};
use piko_protocol::messages::Message;
use piko_protocol::session::SessionTreeEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeEntryKind {
    User,
    Assistant,
    Tool,
    Model,
    Thinking,
    Branch,
    Compaction,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub depth: usize,
    pub label: String,
    pub kind: TreeEntryKind,
    pub on_path: bool,
    pub is_leaf: bool,
    pub has_children: bool,
    pub expanded: bool,
    pub agent_instance_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConversationTreeViewModel {
    pub nodes: Vec<TreeNode>,
    pub current_leaf_id: Option<String>,
    pub preview_entry_id: Option<String>,
}

/// Default expansion: every on-path parent that is a branch point (≥2
/// visible children for `selected_agent`).
pub fn default_tree_expansion(
    entries: &[SessionTreeEntry],
    leaf_id: Option<&str>,
    selected_agent: Option<&str>,
) -> HashSet<String> {
    let path: HashSet<String> = active_path_ids(entries, leaf_id).into_iter().collect();
    let mut child_counts: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        if let (Some(sel), Some(aid)) = (selected_agent, entry_agent_instance(entry).as_deref())
            && aid != sel
        {
            continue;
        }
        if let Some(parent) = entry.parent_id() {
            *child_counts.entry(parent.to_string()).or_default() += 1;
        }
    }
    path.into_iter()
        .filter(|id| child_counts.get(id).copied().unwrap_or(0) >= 2)
        .collect()
}

/// Retain expansion ids that still exist after reconcile.
pub fn prune_tree_expansion(expanded: &mut HashSet<String>, surviving_ids: &HashSet<String>) {
    expanded.retain(|id| surviving_ids.contains(id));
}

/// Derive tree for the selected agent. `preview_entry_id` / `expanded` are GUI-local.
///
/// Display depth follows TUI session-tree rules: indent only at branch points
/// (a visible parent with ≥2 visible children). Single-child chains stay flat.
pub fn derive_conversation_tree(
    state: &ClientState,
    preview_entry_id: Option<&str>,
    expanded: &HashSet<String>,
) -> ConversationTreeViewModel {
    let Some(session) = state.live_session.as_ref() else {
        return ConversationTreeViewModel::default();
    };
    let selected = session.selected_agent.as_deref();
    let path_ids: HashSet<String> =
        active_path_ids(&session.entries, session.current_leaf_id.as_deref())
            .into_iter()
            .collect();
    let leaf = session.current_leaf_id.clone();

    // First pass: agent-filter + drop bookkeeping (match TUI Default tree filter).
    let mut candidates: Vec<TreeNode> = Vec::new();
    for entry in &session.entries {
        if is_tree_bookkeeping(entry) {
            continue;
        }
        let agent_id = entry_agent_instance(entry);
        if let (Some(sel), Some(aid)) = (selected, agent_id.as_deref())
            && aid != sel
        {
            continue;
        }
        let id = entry.id().to_string();
        let parent_id = entry.parent_id().map(str::to_string);
        let (kind, label) = entry_label(entry);
        let on_path = path_ids.contains(&id);
        let is_leaf = leaf.as_deref() == Some(id.as_str());
        candidates.push(TreeNode {
            id,
            parent_id,
            depth: 0,
            label,
            kind,
            on_path,
            is_leaf,
            has_children: false,
            expanded: false,
            agent_instance_id: agent_id,
        });
    }

    // Visible children only (same filter set) — drives branch points + disclosure.
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    let candidate_ids: HashSet<String> = candidates.iter().map(|n| n.id.clone()).collect();
    for node in &candidates {
        if let Some(parent) = &node.parent_id
            && candidate_ids.contains(parent)
        {
            children_of
                .entry(parent.clone())
                .or_default()
                .push(node.id.clone());
        }
    }

    let mut depth_by_id: HashMap<String, usize> = HashMap::new();
    let mut parent_by_id: HashMap<String, Option<String>> = HashMap::new();
    let mut branch_ids: HashSet<String> = HashSet::new();
    for (id, kids) in &children_of {
        if kids.len() >= 2 {
            branch_ids.insert(id.clone());
        }
    }
    for node in &mut candidates {
        parent_by_id.insert(node.id.clone(), node.parent_id.clone());
        let depth = match node.parent_id.as_ref() {
            Some(parent) if depth_by_id.contains_key(parent) => {
                let parent_depth = depth_by_id[parent];
                let branch = branch_ids.contains(parent);
                parent_depth + usize::from(branch)
            }
            _ => 0,
        };
        depth_by_id.insert(node.id.clone(), depth);
        node.depth = depth;
        // Disclosure only at branch points; single-child chains stay open + flat.
        node.has_children = branch_ids.contains(&node.id);
        node.expanded = node.has_children && expanded.contains(&node.id);
    }

    let id_set: HashSet<String> = candidates.iter().map(|n| n.id.clone()).collect();
    let visible: Vec<TreeNode> = candidates
        .into_iter()
        .filter(|node| ancestors_expanded(node, expanded, &branch_ids, &parent_by_id))
        .collect();

    let preview = preview_entry_id
        .filter(|id| id_set.contains(*id))
        .map(str::to_string);

    ConversationTreeViewModel {
        nodes: visible,
        current_leaf_id: leaf,
        preview_entry_id: preview,
    }
}

fn ancestors_expanded(
    node: &TreeNode,
    expanded: &HashSet<String>,
    branch_ids: &HashSet<String>,
    parent_by_id: &HashMap<String, Option<String>>,
) -> bool {
    let mut parent = node.parent_id.clone();
    let mut visited = HashSet::new();
    while let Some(pid) = parent {
        if !visited.insert(pid.clone()) {
            break;
        }
        // Only branch points can collapse descendants.
        if branch_ids.contains(&pid) && !expanded.contains(&pid) {
            return false;
        }
        parent = parent_by_id.get(&pid).cloned().flatten();
    }
    true
}

fn is_tree_bookkeeping(entry: &SessionTreeEntry) -> bool {
    matches!(
        entry,
        SessionTreeEntry::ModelChange(_)
            | SessionTreeEntry::ThinkingLevelChange(_)
            | SessionTreeEntry::ActiveToolsChange(_)
            | SessionTreeEntry::SessionInfo(_)
            | SessionTreeEntry::Custom(_)
            | SessionTreeEntry::Leaf(_)
            | SessionTreeEntry::Label(_)
    )
}

fn entry_agent_instance(entry: &SessionTreeEntry) -> Option<String> {
    match entry {
        SessionTreeEntry::Message(m) => Some(m.agent_instance_id.clone()),
        SessionTreeEntry::ToolCall(t) => t.agent_instance_id.clone(),
        _ => None,
    }
}

fn entry_label(entry: &SessionTreeEntry) -> (TreeEntryKind, String) {
    match entry {
        SessionTreeEntry::Message(m) => {
            let (kind, body) = match &m.message {
                Message::User { content, .. } => {
                    let body = match content {
                        piko_protocol::MessageContent::String(s) => s.clone(),
                        piko_protocol::MessageContent::Blocks(blocks) => blocks
                            .iter()
                            .filter_map(|b| match b {
                                piko_protocol::ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join(" "),
                    };
                    (TreeEntryKind::User, body)
                }
                Message::Assistant { content, .. } => {
                    let text_body = content
                        .iter()
                        .filter_map(|b| match b {
                            piko_protocol::ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    let thinking_body = content
                        .iter()
                        .filter_map(|b| match b {
                            piko_protocol::ContentBlock::Thinking { thinking, .. } => {
                                Some(thinking.as_str())
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !text_body.trim().is_empty() {
                        (TreeEntryKind::Assistant, text_body)
                    } else if !thinking_body.trim().is_empty() {
                        // Thinking-only steps: show thought preview, not "(empty)".
                        (TreeEntryKind::Thinking, thinking_body)
                    } else {
                        (TreeEntryKind::Assistant, String::new())
                    }
                }
                Message::ToolCall { name, .. } => {
                    return (
                        TreeEntryKind::Tool,
                        crate::t!("tree.entry.tool", name = name.as_str()),
                    );
                }
                Message::ToolResult { tool_name, .. } => {
                    let name = tool_name.as_deref().unwrap_or("tool");
                    return (
                        TreeEntryKind::Tool,
                        crate::t!("tree.entry.tool", name = name),
                    );
                }
                other => (TreeEntryKind::Other, other.role().to_string()),
            };
            (kind, truncate(&body, 48))
        }
        SessionTreeEntry::ToolCall(t) => (
            TreeEntryKind::Tool,
            crate::t!("tree.entry.tool", name = t.tool_name.as_str()),
        ),
        SessionTreeEntry::ModelChange(m) => (
            TreeEntryKind::Model,
            crate::t!("tree.entry.model", id = m.model_id.as_str()),
        ),
        SessionTreeEntry::ThinkingLevelChange(t) => (
            TreeEntryKind::Thinking,
            crate::t!("tree.entry.thinking", level = t.thinking_level.as_str()),
        ),
        SessionTreeEntry::BranchSummary(_) => (
            TreeEntryKind::Branch,
            crate::t!("tree.entry.branch_summary"),
        ),
        SessionTreeEntry::Compaction(_) => (
            TreeEntryKind::Compaction,
            crate::t!("tree.entry.compaction"),
        ),
        SessionTreeEntry::Label(l) => (
            TreeEntryKind::Other,
            l.label.clone().unwrap_or_else(|| l.target_id.clone()),
        ),
        other => (TreeEntryKind::Other, other.id().to_string()),
    }
}

fn truncate(s: &str, max: usize) -> String {
    let trimmed = s.trim().replace('\n', " ");
    if trimmed.chars().count() <= max {
        if trimmed.is_empty() {
            crate::t!("tree.entry.no_content")
        } else {
            trimmed
        }
    } else {
        let take: String = trimmed.chars().take(max.saturating_sub(1)).collect();
        format!("{take}…")
    }
}

#[cfg(test)]
#[path = "vm_tests.rs"]
mod tests;
