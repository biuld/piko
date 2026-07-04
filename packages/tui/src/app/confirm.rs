use piko_protocol::{Command, SessionTreeEntry};

use crate::app::{AppMode, AppState, command_id, effect::Effect};

impl AppState {
    pub(crate) fn confirm_summary_prompt(&mut self) -> Vec<Effect> {
        let Some(workflow) = self.summary_prompt.as_mut() else {
            self.pop_focus();
            return Vec::new();
        };

        let confirm = crate::features::tree::confirm_summary_prompt(workflow);
        match confirm {
            crate::features::tree::SummaryPromptConfirm::NeedsInput => Vec::new(),
            crate::features::tree::SummaryPromptConfirm::Navigate {
                entry_id,
                summarize,
                custom_instructions,
            } => {
                self.summary_prompt = None;
                self.pop_focus();
                self.navigate_selected_tree_entry(entry_id, summarize, custom_instructions)
            }
            crate::features::tree::SummaryPromptConfirm::None => {
                self.summary_prompt = None;
                self.pop_focus();
                Vec::new()
            }
        }
    }

    pub(crate) fn confirm_tree_label_edit(&mut self) -> Vec<Effect> {
        if let Some(commit) = self.tree.take_label_edit_commit()
            && let Some(session_id) = &self.session.id
        {
            vec![Effect::send(Command::SessionSetLabel {
                command_id: command_id(),
                session_id: session_id.clone(),
                entry_id: commit.target_id,
                label: commit.label,
            })]
        } else {
            Vec::new()
        }
    }

    pub(crate) fn confirm_tree_entry(&mut self) -> Vec<Effect> {
        let Some(entry_id) = self.tree.selected_filtered_entry_id() else {
            self.status = "no tree entry selected".to_string();
            return Vec::new();
        };
        if Some(&entry_id) == self.tree.document.current_leaf_id.as_ref() {
            self.clear_focus();
            self.status = "already at this point".to_string();
            return Vec::new();
        }

        if self.tree_navigation_needs_summary(&entry_id) && self.summary_prompt.is_none() {
            self.summary_prompt = Some(crate::features::tree::create_summary_prompt(
                entry_id.clone(),
            ));
            self.push_focus(AppMode::SummaryPrompt);
            return Vec::new();
        }

        self.summary_prompt = None;
        self.navigate_selected_tree_entry(entry_id, false, None)
    }

    pub(crate) fn confirm_auth_selection(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        use crate::features::auth_selector::AuthConfirmResult;
        match self.auth_selector.confirm() {
            AuthConfirmResult::StartOAuth { provider } => {
                effects.push(Effect::send(Command::AuthLoginOAuth {
                    command_id: command_id(),
                    provider: provider.clone(),
                }));
                self.status = format!("starting {provider} OAuth login");
                self.pop_focus();
            }
            AuthConfirmResult::StartApiKeyInput => {}
            AuthConfirmResult::SetApiKey { provider, api_key } => {
                effects.push(Effect::send(Command::AuthSetApiKey {
                    command_id: command_id(),
                    provider: provider.clone(),
                    api_key,
                }));
                self.status = format!("API key set for {provider}");
                self.pop_focus();
            }
            AuthConfirmResult::None => {}
        }
        effects
    }

    // ── tree navigation helpers ───────────────────────────────────────────────

    pub(super) fn tree_navigation_needs_summary(&self, selected_entry_id: &str) -> bool {
        let Some(old_leaf_id) = self.tree.document.current_leaf_id.as_deref() else {
            return false;
        };
        let Some(target_id) = self.tree_navigation_target_leaf(selected_entry_id) else {
            return false;
        };
        if target_id.as_deref() == Some(old_leaf_id) {
            return false;
        }

        let entries = &self.tree.document.nodes;
        let active_entries = crate::app::get_active_branch_entries(entries, Some(old_leaf_id));
        if active_entries.is_empty() {
            return false;
        }

        let mut target_ancestors = std::collections::HashSet::new();
        let mut curr = target_id;
        while let Some(id) = curr {
            target_ancestors.insert(id.clone());
            curr = self
                .tree
                .document
                .by_id
                .get(&id)
                .and_then(|idx| entries[*idx].parent_id().map(str::to_string));
        }

        let mut after_common_ancestor = target_ancestors.is_empty();
        for entry in active_entries {
            if target_ancestors.contains(entry.id()) {
                after_common_ancestor = true;
                continue;
            }
            if after_common_ancestor {
                return true;
            }
        }
        false
    }

    pub(super) fn tree_navigation_target_leaf(
        &self,
        selected_entry_id: &str,
    ) -> Option<Option<String>> {
        let entry = self
            .tree
            .document
            .by_id
            .get(selected_entry_id)
            .and_then(|idx| self.tree.document.nodes.get(*idx))?;

        match entry {
            SessionTreeEntry::Message(message) if message.message.role() == "user" => {
                Some(message.parent_id.clone())
            }
            SessionTreeEntry::CustomMessage(message) => Some(message.parent_id.clone()),
            _ => Some(Some(selected_entry_id.to_string())),
        }
    }
}
