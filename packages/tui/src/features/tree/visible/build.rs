use super::*;

impl VisibleTree {
    pub fn build(
        doc: &TreeDocument,
        filter_mode: TreeFilterMode,
        search_query: &str,
        folded: &HashSet<String>,
        agent_filter: Option<&str>,
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
            if let Some(filter) = agent_filter
                && let Some(entry_agent) = entry_agent_instance(entry)
                && entry_agent != filter
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

/// Agent attribution of a session entry, if it belongs to a specific agent
/// instance. Entries without attribution are session-level and stay visible
/// under any agent filter.
fn entry_agent_instance(entry: &SessionTreeEntry) -> Option<&str> {
    match entry {
        SessionTreeEntry::Message(message) => Some(message.agent_instance_id.as_str()),
        SessionTreeEntry::ToolCall(tool_call) => tool_call.agent_instance_id.as_deref(),
        _ => None,
    }
}
