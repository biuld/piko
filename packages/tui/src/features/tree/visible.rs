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

    use piko_protocol::{LeafEntry, Message, MessageContent, MessageEntry, SessionTreeEntry};

    use super::{ConnectorKind, TreeDocument, TreeFilterMode, VisibleTree};

    fn user_entry(id: &str, parent_id: Option<&str>, text: &str) -> SessionTreeEntry {
        SessionTreeEntry::Message(MessageEntry {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: "2026-07-02T00:00:00Z".to_string(),
            agent_id: "main".into(),
            task_id: "task-main".into(),
            work_id: "work-main".into(),
            task_seq: 1,
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

    #[test]
    fn single_child_chain_stays_flat_without_fake_connectors() {
        let entries = vec![
            user_entry("a", None, "a"),
            user_entry("b", Some("a"), "b"),
            user_entry("c", Some("b"), "c"),
        ];
        let doc = TreeDocument::build(&entries, Some("c"));
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new());

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
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new());

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
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new());

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
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new());

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
        let visible = VisibleTree::build(&doc, TreeFilterMode::Default, "", &HashSet::new());

        let future = visible
            .rows
            .iter()
            .find(|row| row.entry_id == "future")
            .expect("future row");
        assert_eq!(future.depth, 0);
        assert_eq!(future.connector, ConnectorKind::None);
        assert!(!future.is_active_path);
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

impl VisibleTree {
    pub fn build(
        doc: &TreeDocument,
        filter_mode: TreeFilterMode,
        search_query: &str,
        folded: &HashSet<String>,
    ) -> Self {
        let mut visible = Self::default();
        let query_tokens: Vec<&str> = search_query.split_whitespace().collect();

        // 1. Filter entries
        let mut visible_entries = Vec::new();
        for entry in &doc.nodes {
            if let SessionTreeEntry::Label(_) = entry
                && filter_mode != TreeFilterMode::All
            {
                continue;
            }

            match filter_mode {
                TreeFilterMode::Default => {
                    if matches!(
                        entry,
                        SessionTreeEntry::ActiveToolsChange(_)
                            | SessionTreeEntry::Custom(_)
                            | SessionTreeEntry::Leaf(_)
                            | SessionTreeEntry::Label(_)
                            | SessionTreeEntry::ModelChange(_)
                            | SessionTreeEntry::ThinkingLevelChange(_)
                            | SessionTreeEntry::SessionInfo(_)
                    ) {
                        continue;
                    }
                }
                TreeFilterMode::NoTools => {
                    if matches!(
                        entry,
                        SessionTreeEntry::ActiveToolsChange(_)
                            | SessionTreeEntry::Custom(_)
                            | SessionTreeEntry::Leaf(_)
                            | SessionTreeEntry::Label(_)
                            | SessionTreeEntry::ModelChange(_)
                            | SessionTreeEntry::ThinkingLevelChange(_)
                            | SessionTreeEntry::SessionInfo(_)
                    ) || matches!(entry, SessionTreeEntry::Message(m) if m.message.role() == "toolResult")
                    {
                        continue;
                    }
                }
                TreeFilterMode::UserOnly => {
                    if !matches!(entry, SessionTreeEntry::Message(m) if m.message.role() == "user")
                    {
                        continue;
                    }
                }
                TreeFilterMode::LabeledOnly => {
                    if !doc.labels_by_target.contains_key(entry.id()) {
                        continue;
                    }
                }
                TreeFilterMode::All => {}
            }

            let text_preview = crate::features::tree::session_entry_preview_text(entry);
            let role_preview = crate::features::tree::session_entry_label(entry);

            let label = doc.labels_by_target.get(entry.id()).cloned();

            if !query_tokens.is_empty() {
                let search_target = format!(
                    "{} {} {}",
                    text_preview,
                    role_preview,
                    label
                        .as_ref()
                        .and_then(|l| l.text.as_deref())
                        .unwrap_or_default()
                )
                .to_lowercase();
                if !query_tokens
                    .iter()
                    .all(|t| search_target.contains(&t.to_lowercase()))
                {
                    continue;
                }
            }

            visible_entries.push((entry.id().to_string(), text_preview, role_preview, label));
        }

        // 2. Build visible hierarchy
        let visible_set: HashSet<String> = visible_entries
            .iter()
            .map(|(id, _, _, _)| id.clone())
            .collect();
        let mut nearest_visible_ancestor = HashMap::new();

        for entry in &doc.nodes {
            let id = entry.id().to_string();
            let mut curr = entry.parent_id().map(str::to_string);
            while let Some(pid) = curr {
                if visible_set.contains(&pid) {
                    nearest_visible_ancestor.insert(id.clone(), pid);
                    break;
                }
                if let Some(&idx) = doc.by_id.get(&pid) {
                    curr = doc.nodes[idx].parent_id().map(str::to_string);
                } else {
                    break;
                }
            }
        }

        for id in &visible_set {
            let parent = nearest_visible_ancestor.get(id).cloned();
            visible.parent_by_id.insert(id.clone(), parent.clone());
            visible
                .children_by_id
                .entry(parent)
                .or_default()
                .push(id.clone());
        }

        // 3. DFS to build rows with depth and connectors
        let mut roots = visible
            .children_by_id
            .get(&None)
            .cloned()
            .unwrap_or_default();
        roots.sort_by_key(|id| {
            (
                !doc.active_path.contains(id),
                doc.by_id.get(id).copied().unwrap_or(0),
            )
        });
        let multiple_roots = roots.len() > 1;

        let mut visible_entries_map = HashMap::new();
        for (id, t, r, l) in visible_entries {
            visible_entries_map.insert(id, (t, r, l));
        }

        #[allow(clippy::too_many_arguments)]
        fn dfs(
            id: &str,
            indent: usize,
            just_branched: bool,
            show_connector: bool,
            is_last: bool,
            gutters: &[Gutter],
            is_virtual_root_child: bool,
            visible: &mut VisibleTree,
            doc: &TreeDocument,
            folded: &HashSet<String>,
            visible_entries_map: &HashMap<String, (String, String, Option<TreeLabel>)>,
        ) {
            let (text_preview, role_preview, label) = visible_entries_map.get(id).unwrap().clone();

            let is_folded = folded.contains(id);
            let connector_displayed = show_connector && !is_virtual_root_child;
            let connector = if connector_displayed {
                if is_last {
                    ConnectorKind::Corner
                } else {
                    ConnectorKind::Branch
                }
            } else {
                ConnectorKind::None
            };

            visible.rows.push(TreeRow {
                entry_id: id.to_string(),
                depth: indent,
                connector,
                gutters: gutters.to_vec(),
                is_active_path: doc.active_path.contains(id),
                is_folded,
                label,
                text_preview,
                role_preview,
            });

            if is_folded {
                return;
            }

            if let Some(children) = visible.children_by_id.get(&Some(id.to_string())) {
                let mut sorted_children = children.clone();
                sorted_children.sort_by_key(|cid| {
                    (
                        !doc.active_path.contains(cid),
                        doc.by_id.get(cid).copied().unwrap_or(0),
                    )
                });

                let branches_children = sorted_children.len() > 1;
                let child_indent = if branches_children || (just_branched && indent > 0) {
                    indent + 1
                } else {
                    indent
                };

                let mut child_gutters = gutters.to_vec();
                if connector_displayed {
                    child_gutters.push(Gutter {
                        position: indent.saturating_sub(1),
                        kind: if is_last {
                            ConnectorKind::None
                        } else {
                            ConnectorKind::Vertical
                        },
                    });
                }

                for (i, child_id) in sorted_children.iter().enumerate() {
                    let child_is_last = i == sorted_children.len() - 1;
                    dfs(
                        child_id,
                        child_indent,
                        branches_children,
                        branches_children,
                        child_is_last,
                        &child_gutters,
                        false,
                        visible,
                        doc,
                        folded,
                        visible_entries_map,
                    );
                }
            }
        }

        for (i, root) in roots.iter().enumerate() {
            let is_last = i == roots.len() - 1;
            dfs(
                root,
                if multiple_roots { 1 } else { 0 },
                multiple_roots,
                multiple_roots,
                is_last,
                &[],
                false, // Enable connectors for multiple roots to preserve tree structure when common ancestor is filtered out
                &mut visible,
                doc,
                folded,
                &visible_entries_map,
            );
        }

        visible
    }
}
