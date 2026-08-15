use super::document::{TreeDocument, TreeLabel};
use piko_protocol::SessionTreeEntry;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeFilterMode {
    #[default]
    Default,
    NoTools,
    UserOnly,
    LabeledOnly,
    All,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use piko_protocol::{
        CompactionEntry, LeafEntry, Message, MessageContent, MessageEntry, SessionTreeEntry,
        ToolCallEntry,
    };

    use super::{ConnectorKind, TreeDocument, TreeFilterMode, VisibleTree};

    fn user_entry(id: &str, parent_id: Option<&str>, text: &str) -> SessionTreeEntry {
        message_entry(id, parent_id, "task-main", text)
    }

    fn message_entry(
        id: &str,
        parent_id: Option<&str>,
        agent_instance_id: &str,
        text: &str,
    ) -> SessionTreeEntry {
        SessionTreeEntry::Message(MessageEntry {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: "2026-07-02T00:00:00Z".to_string(),
            agent_id: "main".into(),
            agent_instance_id: agent_instance_id.into(),
            source_turn_id: "work-main".into(),
            transcript_seq: 1,
            message: Message::User {
                content: MessageContent::String(text.to_string()),
                timestamp: None,
            },
        })
    }

    fn leaf_entry(id: &str, parent_id: Option<&str>, target_id: Option<&str>) -> SessionTreeEntry {
        SessionTreeEntry::Leaf(LeafEntry {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: "2026-07-02T00:00:00Z".to_string(),
            target_id: target_id.map(str::to_string),
        })
    }

    fn compaction_entry(id: &str, parent_id: Option<&str>) -> SessionTreeEntry {
        SessionTreeEntry::Compaction(CompactionEntry {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: "2026-07-02T00:00:00Z".to_string(),
            summary: "compacted".to_string(),
            first_kept_entry_id: "a".to_string(),
            tokens_before: 100,
            details: None,
            from_hook: None,
        })
    }

    fn tool_call_entry(
        id: &str,
        parent_id: Option<&str>,
        agent_instance_id: Option<&str>,
        tool_name: &str,
    ) -> SessionTreeEntry {
        SessionTreeEntry::ToolCall(ToolCallEntry {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: "2026-07-02T00:00:00Z".to_string(),
            agent_id: Some("main".into()),
            agent_instance_id: agent_instance_id.map(str::to_string),
            tool_call_id: format!("{id}-call"),
            tool_name: tool_name.to_string(),
            arguments: serde_json::json!({}),
            parent_message_id: None,
            model: None,
            provider: None,
        })
    }

    #[test]
    fn single_child_chain_stays_flat_without_fake_connectors() {
        let entries = vec![
            user_entry("a", None, "a"),
            user_entry("b", Some("a"), "b"),
            user_entry("c", Some("b"), "c"),
        ];
        let doc = TreeDocument::build(&entries, Some("c"));
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new(), None);

        assert_eq!(visible.rows.len(), 3);
        assert!(
            visible
                .rows
                .iter()
                .all(|row| row.depth == 0 && row.connector == ConnectorKind::None)
        );
    }

    #[test]
    fn branch_point_uses_branch_and_corner_connectors() {
        let entries = vec![
            user_entry("root", None, "root"),
            user_entry("left", Some("root"), "left"),
            user_entry("right", Some("root"), "right"),
        ];
        let doc = TreeDocument::build(&entries, Some("left"));
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new(), None);

        assert_eq!(visible.rows[0].connector, ConnectorKind::None);
        assert_eq!(visible.rows[1].depth, 1);
        assert_eq!(visible.rows[1].connector, ConnectorKind::Branch);
        assert_eq!(visible.rows[2].depth, 1);
        assert_eq!(visible.rows[2].connector, ConnectorKind::Corner);
    }

    #[test]
    fn default_filter_hides_leaf_entries_so_navigation_cursors_do_not_create_branches() {
        let entries = vec![
            user_entry("root", None, "root"),
            user_entry("child", Some("root"), "child"),
            leaf_entry("leaf-a", Some("child"), Some("root")),
            leaf_entry("leaf-b", Some("leaf-a"), Some("child")),
        ];
        let doc = TreeDocument::build(&entries, Some("child"));
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new(), None);

        assert_eq!(
            visible
                .rows
                .iter()
                .map(|row| row.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["root", "child"]
        );
        assert!(
            visible
                .rows
                .iter()
                .all(|row| row.connector == ConnectorKind::None)
        );
    }

    #[test]
    fn active_branch_sorts_before_sibling_branches() {
        let entries = vec![
            user_entry("root", None, "root"),
            user_entry("inactive", Some("root"), "inactive"),
            user_entry("active", Some("root"), "active"),
        ];
        let doc = TreeDocument::build(&entries, Some("active"));
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new(), None);

        assert_eq!(
            visible
                .rows
                .iter()
                .map(|row| row.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["root", "active", "inactive"]
        );
    }

    #[test]
    fn child_leaving_active_path_stays_flat_when_it_is_only_visible_child() {
        let entries = vec![
            user_entry("root", None, "root"),
            user_entry("active-leaf", Some("root"), "active"),
            user_entry("future", Some("active-leaf"), "future"),
        ];
        let doc = TreeDocument::build(&entries, Some("active-leaf"));
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new(), None);

        let future = visible
            .rows
            .iter()
            .find(|row| row.entry_id == "future")
            .expect("future row");
        assert_eq!(future.depth, 0);
        assert_eq!(future.connector, ConnectorKind::None);
        assert!(!future.is_active_path);
    }

    #[test]
    fn agent_filter_hides_other_agent_entries_and_keeps_session_level_entries() {
        let entries = vec![
            user_entry("root", None, "root"),
            message_entry("sub", Some("root"), "task-child", "sub"),
            compaction_entry("compaction", Some("root")),
            tool_call_entry("tool", Some("root"), None, "read_file"),
        ];
        let doc = TreeDocument::build(&entries, Some("root"));

        let root_visible = VisibleTree::build(
            &doc,
            TreeFilterMode::Default,
            "",
            &HashSet::new(),
            Some("task-main"),
        );
        assert_eq!(
            root_visible
                .rows
                .iter()
                .map(|row| row.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["root", "compaction", "tool"]
        );

        let child_visible = VisibleTree::build(
            &doc,
            TreeFilterMode::Default,
            "",
            &HashSet::new(),
            Some("task-child"),
        );
        assert_eq!(
            child_visible
                .rows
                .iter()
                .map(|row| row.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["sub", "compaction", "tool"]
        );
    }

    #[test]
    fn agent_filter_reparents_entries_whose_parent_belongs_to_another_agent() {
        let entries = vec![
            user_entry("root", None, "root"),
            tool_call_entry("spawn", Some("root"), Some("task-main"), "spawn_agent"),
            message_entry("sub", Some("spawn"), "task-child", "sub"),
            tool_call_entry("sub-tool", Some("sub"), Some("task-child"), "read_file"),
        ];
        let doc = TreeDocument::build(&entries, Some("sub"));
        let visible = VisibleTree::build(
            &doc,
            TreeFilterMode::Default,
            "",
            &HashSet::new(),
            Some("task-child"),
        );

        assert_eq!(
            visible
                .rows
                .iter()
                .map(|row| row.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["sub", "sub-tool"]
        );
        assert!(
            visible
                .rows
                .iter()
                .all(|row| row.depth == 0 && row.connector == ConnectorKind::None)
        );
    }
}

impl From<crate::config::TreeFilterMode> for TreeFilterMode {
    fn from(mode: crate::config::TreeFilterMode) -> Self {
        match mode {
            crate::config::TreeFilterMode::Default => Self::Default,
            crate::config::TreeFilterMode::NoTools => Self::NoTools,
            crate::config::TreeFilterMode::UserOnly => Self::UserOnly,
            crate::config::TreeFilterMode::LabeledOnly => Self::LabeledOnly,
            crate::config::TreeFilterMode::All => Self::All,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectorKind {
    None,
    Vertical,
    Branch,
    Corner,
}

#[derive(Clone, Debug)]
pub struct Gutter {
    pub position: usize,
    pub kind: ConnectorKind,
}

#[derive(Clone, Debug)]
pub struct TreeRow {
    pub entry_id: String,
    pub depth: usize,
    pub connector: ConnectorKind,
    pub gutters: Vec<Gutter>,
    pub is_active_path: bool,
    pub is_folded: bool,
    pub label: Option<TreeLabel>,
    pub text_preview: String,
    pub role_preview: String,
}

#[derive(Default, Debug)]
pub struct VisibleTree {
    pub rows: Vec<TreeRow>,
    pub parent_by_id: HashMap<String, Option<String>>,
    pub children_by_id: HashMap<Option<String>, Vec<String>>,
}

mod build;
