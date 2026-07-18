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

/// Default expansion: every on-path parent that has children.
pub fn default_tree_expansion(
    entries: &[SessionTreeEntry],
    leaf_id: Option<&str>,
) -> HashSet<String> {
    let path: HashSet<String> = active_path_ids(entries, leaf_id).into_iter().collect();
    let mut child_counts: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        if let Some(parent) = entry.parent_id() {
            *child_counts.entry(parent.to_string()).or_default() += 1;
        }
    }
    path.into_iter()
        .filter(|id| child_counts.get(id).copied().unwrap_or(0) > 0)
        .collect()
}

/// Retain expansion ids that still exist after reconcile.
pub fn prune_tree_expansion(expanded: &mut HashSet<String>, surviving_ids: &HashSet<String>) {
    expanded.retain(|id| surviving_ids.contains(id));
}

/// Derive tree for the selected agent. `preview_entry_id` / `expanded` are GUI-local.
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

    let mut all_nodes = Vec::new();
    let mut depth_by_id: HashMap<String, usize> = HashMap::new();
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();

    for entry in &session.entries {
        let id = entry.id().to_string();
        let parent_id = entry.parent_id().map(str::to_string);
        let depth = parent_id
            .as_ref()
            .and_then(|p| depth_by_id.get(p).copied())
            .map(|d| d + 1)
            .unwrap_or(0);
        depth_by_id.insert(id.clone(), depth);
        if let Some(parent) = parent_id.clone() {
            children_of.entry(parent).or_default().push(id.clone());
        }

        let agent_id = entry_agent_instance(entry);
        if let (Some(sel), Some(aid)) = (selected, agent_id.as_deref())
            && aid != sel
        {
            continue;
        }

        let (kind, label) = entry_label(entry);
        let on_path = path_ids.contains(&id);
        let is_leaf = leaf.as_deref() == Some(id.as_str());
        all_nodes.push(TreeNode {
            id,
            parent_id,
            depth,
            label,
            kind,
            on_path,
            is_leaf,
            has_children: false,
            expanded: false,
            agent_instance_id: agent_id,
        });
    }

    for node in &mut all_nodes {
        let count = children_of.get(&node.id).map(|c| c.len()).unwrap_or(0);
        node.has_children = count > 0;
        node.expanded = node.has_children && expanded.contains(&node.id);
    }

    let id_set: HashSet<String> = all_nodes.iter().map(|n| n.id.clone()).collect();
    let visible: Vec<TreeNode> = all_nodes
        .into_iter()
        .filter(|node| ancestors_expanded(node, expanded, &session.entries))
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
    entries: &[SessionTreeEntry],
) -> bool {
    let mut parent = node.parent_id.clone();
    let by_id: HashMap<&str, &SessionTreeEntry> = entries.iter().map(|e| (e.id(), e)).collect();
    while let Some(pid) = parent {
        if !expanded.contains(&pid) {
            return false;
        }
        parent = by_id
            .get(pid.as_str())
            .and_then(|e| e.parent_id().map(str::to_string));
    }
    true
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
                    let body = content
                        .iter()
                        .filter_map(|b| match b {
                            piko_protocol::ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    (TreeEntryKind::Assistant, body)
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
            "(empty)".into()
        } else {
            trimmed
        }
    } else {
        let take: String = trimmed.chars().take(max.saturating_sub(1)).collect();
        format!("{take}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_client_core::SessionPhase;
    use piko_client_core::state::LiveSession;
    use piko_protocol::session::MessageEntry;

    fn msg(id: &str, parent: Option<&str>, agent: &str, text: &str) -> SessionTreeEntry {
        SessionTreeEntry::Message(MessageEntry {
            id: id.into(),
            parent_id: parent.map(str::to_string),
            timestamp: "1".into(),
            agent_id: "spec".into(),
            agent_instance_id: agent.into(),
            source_turn_id: "t".into(),
            transcript_seq: 1,
            message: Message::User {
                content: piko_protocol::MessageContent::String(text.into()),
                timestamp: Some(1),
            },
        })
    }

    #[test]
    fn marks_active_path_and_off_path_preview() {
        let mut state = ClientState::default();
        state.session_phase = SessionPhase::Live;
        let entries = vec![
            msg("a", None, "root", "first"),
            msg("b", Some("a"), "root", "second"),
            msg("c", Some("a"), "root", "branch"),
        ];
        state.live_session = Some(LiveSession {
            session_id: "s1".into(),
            selected_agent: Some("root".into()),
            current_leaf_id: Some("b".into()),
            entries: entries.clone(),
            ..Default::default()
        });

        let expanded = default_tree_expansion(&entries, Some("b"));
        let vm = derive_conversation_tree(&state, Some("c"), &expanded);
        assert!(vm.nodes.iter().any(|n| n.id == "a"));
        assert!(vm.nodes.iter().any(|n| n.id == "b"));
        assert!(vm.nodes.iter().any(|n| n.id == "c"));
        let a = vm.nodes.iter().find(|n| n.id == "a").unwrap();
        let b = vm.nodes.iter().find(|n| n.id == "b").unwrap();
        let c = vm.nodes.iter().find(|n| n.id == "c").unwrap();
        assert!(a.on_path && b.on_path && !c.on_path);
        assert!(b.is_leaf);
        assert_eq!(vm.preview_entry_id.as_deref(), Some("c"));
    }

    #[test]
    fn on_path_preview_stays_display_only() {
        let mut state = ClientState::default();
        state.session_phase = SessionPhase::Live;
        let entries = vec![
            msg("a", None, "root", "first"),
            msg("b", Some("a"), "root", "second"),
        ];
        state.live_session = Some(LiveSession {
            session_id: "s1".into(),
            selected_agent: Some("root".into()),
            current_leaf_id: Some("b".into()),
            entries: entries.clone(),
            ..Default::default()
        });
        let expanded = default_tree_expansion(&entries, Some("b"));
        let vm = derive_conversation_tree(&state, Some("a"), &expanded);
        assert_eq!(vm.preview_entry_id.as_deref(), Some("a"));
    }

    #[test]
    fn collapsed_parent_hides_children() {
        let mut state = ClientState::default();
        state.session_phase = SessionPhase::Live;
        let entries = vec![
            msg("a", None, "root", "first"),
            msg("b", Some("a"), "root", "second"),
            msg("c", Some("a"), "root", "branch"),
        ];
        state.live_session = Some(LiveSession {
            session_id: "s1".into(),
            selected_agent: Some("root".into()),
            current_leaf_id: Some("b".into()),
            entries,
            ..Default::default()
        });
        let expanded = HashSet::new();
        let vm = derive_conversation_tree(&state, None, &expanded);
        assert_eq!(vm.nodes.len(), 1);
        assert_eq!(vm.nodes[0].id, "a");
        assert!(vm.nodes[0].has_children);
        assert!(!vm.nodes[0].expanded);
    }

    #[test]
    fn prune_keeps_surviving_expansion_ids() {
        let mut expanded: HashSet<String> = ["a", "gone"].into_iter().map(str::to_string).collect();
        let surviving: HashSet<String> = ["a", "b"].into_iter().map(str::to_string).collect();
        prune_tree_expansion(&mut expanded, &surviving);
        assert_eq!(expanded, ["a".into()].into_iter().collect());
    }

    #[test]
    fn message_tool_call_uses_tool_kind_for_wrench_icon() {
        crate::i18n::init();
        let entry = SessionTreeEntry::Message(MessageEntry {
            id: "tc".into(),
            parent_id: None,
            timestamp: "1".into(),
            agent_id: "spec".into(),
            agent_instance_id: "root".into(),
            source_turn_id: "t".into(),
            transcript_seq: 1,
            message: Message::ToolCall {
                id: "call-1".into(),
                name: "bash".into(),
                arguments: serde_json::json!({}),
                model: None,
                provider: None,
                timestamp: Some(1),
            },
        });
        let (kind, label) = entry_label(&entry);
        assert_eq!(kind, TreeEntryKind::Tool);
        assert!(label.contains("bash"));
    }
}
