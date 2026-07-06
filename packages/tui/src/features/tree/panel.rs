use piko_protocol::SessionTreeEntry;
use std::collections::HashSet;

use super::{LabelEditorState, TreeDocument, TreeFilterMode, TreePanel, VisibleTree};
use crate::ui::components::text_box::TextBox;

pub struct LabelEditCommit {
    pub target_id: String,
    pub label: Option<String>,
}

impl TreePanel {
    pub fn new() -> Self {
        Self {
            document: TreeDocument::default(),
            visible: VisibleTree::default(),
            selection: None,
            filter: String::new(),
            filter_mode: TreeFilterMode::Default,
            folded: HashSet::new(),
            show_label_timestamps: false,
            label_editor: None,
            selected_idx: 0,
        }
    }

    pub fn load(&mut self, snapshot_entries: &[SessionTreeEntry], current_leaf_id: Option<&str>) {
        self.document = TreeDocument::build(snapshot_entries, current_leaf_id);
        self.rebuild_visible("");
        self.select_nearest_visible(current_leaf_id);
    }

    pub fn select_entry(&mut self, entry_id: &str) {
        if let Some(idx) = self
            .visible
            .rows
            .iter()
            .position(|r| r.entry_id == entry_id)
        {
            self.selected_idx = idx;
            self.selection = Some(entry_id.to_string());
        }
    }

    pub fn rebuild_visible(&mut self, search_query: &str) {
        self.visible =
            VisibleTree::build(&self.document, self.filter_mode, search_query, &self.folded);

        // Nearest-visible selection fallback
        if let Some(sel) = self.selection.clone() {
            if !self.visible.rows.iter().any(|r| r.entry_id == sel) {
                // Not visible anymore, walk up parents
                let mut curr = self
                    .document
                    .by_id
                    .get(&sel)
                    .and_then(|&idx| self.document.nodes[idx].parent_id().map(str::to_string));
                let mut found = false;
                while let Some(pid) = curr {
                    if self.visible.rows.iter().any(|r| r.entry_id == pid) {
                        self.select_entry(&pid);
                        found = true;
                        break;
                    }
                    if let Some(&idx) = self.document.by_id.get(&pid) {
                        curr = self.document.nodes[idx].parent_id().map(str::to_string);
                    } else {
                        break;
                    }
                }
                if !found {
                    self.selected_idx = self.visible.rows.len().saturating_sub(1);
                    self.selection = self
                        .visible
                        .rows
                        .get(self.selected_idx)
                        .map(|r| r.entry_id.clone());
                }
            } else {
                self.select_entry(&sel); // Updates selected_idx
            }
        }
    }

    pub fn rebuild_visible_for_filter(&mut self) {
        let filter = self.filter.clone();
        self.rebuild_visible(&filter);
    }

    fn select_nearest_visible(&mut self, entry_id: Option<&str>) {
        if self.visible.rows.is_empty() {
            self.selected_idx = 0;
            self.selection = None;
            return;
        }

        let mut curr = entry_id.map(str::to_string);
        while let Some(id) = curr {
            if self.visible.rows.iter().any(|row| row.entry_id == id) {
                self.select_entry(&id);
                return;
            }
            curr = self
                .document
                .by_id
                .get(&id)
                .and_then(|idx| self.document.nodes[*idx].parent_id().map(str::to_string));
        }

        self.selected_idx = self.visible.rows.len().saturating_sub(1);
        self.selection = self
            .visible
            .rows
            .get(self.selected_idx)
            .map(|row| row.entry_id.clone());
    }

    pub fn select_next(&mut self, search_query: &str) {
        self.rebuild_visible(search_query);
        if self.visible.rows.is_empty() {
            return;
        }
        if self.selected_idx + 1 < self.visible.rows.len() {
            self.selected_idx += 1;
            self.selection = Some(self.visible.rows[self.selected_idx].entry_id.clone());
        }
    }

    pub fn select_next_filtered(&mut self) {
        let filter = self.filter.clone();
        self.select_next(&filter);
    }

    pub fn select_prev(&mut self, search_query: &str) {
        self.rebuild_visible(search_query);
        if self.visible.rows.is_empty() {
            return;
        }
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            self.selection = Some(self.visible.rows[self.selected_idx].entry_id.clone());
        }
    }

    pub fn select_prev_filtered(&mut self) {
        let filter = self.filter.clone();
        self.select_prev(&filter);
    }

    pub fn selected_entry_id(&self, _search_query: &str) -> Option<String> {
        self.selection.clone()
    }

    pub fn selected_filtered_entry_id(&self) -> Option<String> {
        self.selected_entry_id(&self.filter)
    }

    pub fn begin_label_edit(&mut self) -> bool {
        let Some(id) = self.selected_filtered_entry_id() else {
            return false;
        };
        self.label_editor = Some(LabelEditorState {
            target_id: id,
            input: TextBox::new(),
        });
        true
    }

    pub fn cancel_label_edit(&mut self) {
        self.label_editor = None;
    }

    pub fn take_label_edit_commit(&mut self) -> Option<LabelEditCommit> {
        let state = self.label_editor.take()?;
        let label = if state.input.text().trim().is_empty() {
            None
        } else {
            Some(state.input.text().to_string())
        };
        Some(LabelEditCommit {
            target_id: state.target_id,
            label,
        })
    }

    pub fn fold_or_up(&mut self, search_query: &str) {
        self.rebuild_visible(search_query);
        let Some(sel) = &self.selection else { return };

        if let Some(children) = self.visible.children_by_id.get(&Some(sel.clone()))
            && !children.is_empty()
            && !self.folded.contains(sel)
        {
            // Is foldable and not folded -> fold it
            self.folded.insert(sel.clone());
            self.rebuild_visible(search_query);
            return;
        }

        // Else move to previous branch segment start
        let mut curr = self.visible.parent_by_id.get(sel).cloned().flatten();
        while let Some(pid) = &curr {
            if self
                .visible
                .children_by_id
                .get(&self.visible.parent_by_id.get(pid).cloned().flatten())
                .map_or(0, |c| c.len())
                > 1
                || !self.visible.parent_by_id.contains_key(pid)
            {
                self.select_entry(pid);
                break;
            }
            curr = self.visible.parent_by_id.get(pid).cloned().flatten();
        }
    }

    pub fn fold_or_up_filtered(&mut self) {
        let filter = self.filter.clone();
        self.fold_or_up(&filter);
    }

    pub fn unfold_or_down(&mut self, search_query: &str) {
        self.rebuild_visible(search_query);
        let Some(sel) = &self.selection else { return };

        if self.folded.contains(sel) {
            self.folded.remove(sel);
            self.rebuild_visible(search_query);
            return;
        }

        // Move to next branch segment start or current branch end
        let mut curr = self
            .visible
            .children_by_id
            .get(&Some(sel.clone()))
            .and_then(|c| c.first().cloned());
        while let Some(cid) = &curr {
            if self
                .visible
                .children_by_id
                .get(&Some(cid.clone()))
                .map_or(0, |c| c.len())
                > 1
                || self
                    .visible
                    .children_by_id
                    .get(&Some(cid.clone()))
                    .map_or(0, |c| c.len())
                    == 0
            {
                self.select_entry(cid);
                break;
            }
            curr = self
                .visible
                .children_by_id
                .get(&Some(cid.clone()))
                .and_then(|c| c.first().cloned());
        }
    }

    pub fn unfold_or_down_filtered(&mut self) {
        let filter = self.filter.clone();
        self.unfold_or_down(&filter);
    }

    pub fn toggle_filter(&mut self, mode: TreeFilterMode, search_query: &str) {
        if self.filter_mode == mode {
            self.filter_mode = TreeFilterMode::Default;
        } else {
            self.filter_mode = mode;
        }
        self.folded.clear(); // Changing filter resets folding
        self.rebuild_visible(search_query);
    }

    pub fn toggle_filter_for_current_search(&mut self, mode: TreeFilterMode) {
        let filter = self.filter.clone();
        self.toggle_filter(mode, &filter);
    }
}

#[cfg(test)]
mod tests {
    use super::TreePanel;
    use piko_protocol::{LeafEntry, Message, MessageContent, MessageEntry, SessionTreeEntry};

    #[test]
    fn load_selects_nearest_visible_ancestor_when_current_leaf_is_hidden() {
        let entries = vec![
            SessionTreeEntry::Message(MessageEntry {
                id: "root".into(),
                parent_id: None,
                timestamp: "2026-07-02T00:00:00Z".into(),
                agent_id: None,
                task_id: None,
                message: Message::User {
                    content: MessageContent::String("root".into()),
                    timestamp: None,
                },
            }),
            SessionTreeEntry::Leaf(LeafEntry {
                id: "hidden-leaf".into(),
                parent_id: Some("root".into()),
                timestamp: "2026-07-02T00:00:01Z".into(),
                target_id: Some("root".into()),
            }),
        ];
        let mut panel = TreePanel::new();

        panel.load(&entries, Some("hidden-leaf"));

        assert_eq!(panel.selection.as_deref(), Some("root"));
    }
}
