use piko_protocol::{Command, SessionTreeEntry};

use crate::app::{
    AppMode, AppState,
    command::{
        Action, AppAction, ApprovalAction, EditorAction, ModelAction, NotificationAction,
        SessionAction, SlashAction, SurfaceAction, TimelineAction, ToolInteractionAction,
        TreeAction,
    },
    command_id,
    effect::Effect,
};

impl AppState {
    // ── action dispatch ───────────────────────────────────────────────────────

    /// Main entry point for all intents.  Called from `main.rs` after
    /// translating raw key events or surface selections into `Action` values.
    pub fn dispatch(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::App(action) => self.dispatch_app_action(action),
            Action::Editor(action) => self.dispatch_editor_action(action),
            Action::Timeline(action) => self.dispatch_timeline_action(action),
            Action::Surface(action) => self.dispatch_surface_action(action),
            Action::Session(action) => self.dispatch_session_action(action),
            Action::Model(action) => self.dispatch_model_action(action),
            Action::Tree(action) => self.dispatch_tree_action(action),
            Action::Approval(action) => self.dispatch_approval_action(action),
            Action::ToolInteraction(action) => self.dispatch_tool_interaction_action(action),
            Action::Notifications(action) => self.dispatch_notification_action(action),
            Action::Slash(action) => self.dispatch_slash_action(action),
        }
    }

    fn dispatch_app_action(&mut self, action: AppAction) -> Vec<Effect> {
        match action {
            AppAction::Quit => self.quit = true,
        }
        Vec::new()
    }

    fn dispatch_editor_action(&mut self, action: EditorAction) -> Vec<Effect> {
        let mut effects = Vec::new();
        match action {
            EditorAction::Submit => effects.extend(self.submit()),
            EditorAction::Cancel => effects.extend(self.cancel()),
            EditorAction::CancelSuggestions => self.editor.auto_complete.clear(),
            EditorAction::InsertChar(ch) => {
                self.editor.insert_char(ch);
                self.refresh_suggestions();
            }
            EditorAction::InsertPaste(text) => {
                if let Some(tb) = self.active_text_box() {
                    tb.insert_str(&text);
                } else {
                    self.editor.insert_paste(&text, &self.tui_config.editor);
                    self.refresh_suggestions();
                }
            }
            EditorAction::InsertNewline => {
                self.editor.insert_newline();
                self.refresh_suggestions();
            }
            EditorAction::DeleteBackward => {
                self.editor.backspace();
                self.refresh_suggestions();
            }
            EditorAction::DeleteForward => {
                self.editor.delete();
                self.refresh_suggestions();
            }
            EditorAction::CursorLeft => {
                self.editor.move_left();
                self.refresh_suggestions();
            }
            EditorAction::CursorRight => {
                self.editor.move_right();
                self.refresh_suggestions();
            }
            EditorAction::CursorLineStart => {
                self.editor.move_line_start();
                self.refresh_suggestions();
            }
            EditorAction::CursorLineEnd => {
                self.editor.move_line_end();
                self.refresh_suggestions();
            }
            EditorAction::HistoryPrev => self.history_prev(),
            EditorAction::HistoryNext => self.history_next(),
            EditorAction::AcceptSuggestion => self.accept_suggestion(),
            EditorAction::AcceptAndSubmitSuggestion => {
                let keep_active = self
                    .editor
                    .auto_complete
                    .items
                    .get(self.editor.auto_complete.selected)
                    .is_some_and(|item| item.keep_active);
                self.accept_suggestion();
                if !keep_active {
                    effects.extend(self.submit());
                }
            }
            EditorAction::SuggestionSelectNext => self.select_suggestion_next(),
            EditorAction::SuggestionSelectPrev => self.select_suggestion_prev(),
            EditorAction::OpenCommands => {
                self.focus_manager.clear_to_chat();
                self.mode = AppMode::Chat;
                let text = self.editor.text();
                if !text.starts_with('/') {
                    self.editor.insert_char('/');
                    self.refresh_suggestions();
                }
                self.status = "commands".to_string();
            }
        }
        effects
    }

    fn dispatch_timeline_action(&mut self, action: TimelineAction) -> Vec<Effect> {
        match action {
            TimelineAction::ScrollUp(n) => self.timeline.scroll_up(n),
            TimelineAction::ScrollDown(n) => self.timeline.scroll_down(n),
            TimelineAction::JumpLatest => self.timeline.jump_latest(),
        }
        Vec::new()
    }

    fn dispatch_surface_action(&mut self, action: SurfaceAction) -> Vec<Effect> {
        match action {
            SurfaceAction::OpenHelp => {
                self.push_focus(AppMode::Help);
                self.status = "help".to_string();
            }
            SurfaceAction::OpenSettings => {
                self.settings.open_root();
                self.push_focus(AppMode::Settings);
                self.status = "settings".to_string();
            }
            SurfaceAction::OpenStatus => {
                self.push_focus(AppMode::Status);
                self.status = "status".to_string();
            }
            SurfaceAction::OpenTree => {
                self.tree.filter_mode = self.tui_config.tree.filter_mode.into();
                self.push_focus(AppMode::Tree);
                self.tree.rebuild_visible_for_filter();
                self.status = format!("{} session entries", self.tree.visible.rows.len());
            }
            SurfaceAction::OpenThinking => {
                self.settings.open_thinking();
                self.push_focus(AppMode::Settings);
                self.status = "thinking level".to_string();
            }
            SurfaceAction::Close => self.close_surface(),
            SurfaceAction::SelectNext => self.select_surface_next(),
            SurfaceAction::SelectPrev => self.select_surface_prev(),
            SurfaceAction::Confirm => return self.confirm_selection(),
            SurfaceAction::FilterAppend(ch) => self.append_active_filter(ch),
            SurfaceAction::FilterBackspace => self.backspace_active_filter(),
        }
        Vec::new()
    }

    fn dispatch_session_action(&mut self, action: SessionAction) -> Vec<Effect> {
        let mut effects = Vec::new();
        match action {
            SessionAction::RequestList => effects.extend(self.request_sessions()),
            SessionAction::ToggleScope => {
                if self.mode == AppMode::Sessions {
                    self.sessions.scope = match self.sessions.scope {
                        crate::features::session_list::SessionScope::CurrentFolder => {
                            crate::features::session_list::SessionScope::All
                        }
                        crate::features::session_list::SessionScope::All => {
                            crate::features::session_list::SessionScope::CurrentFolder
                        }
                    };
                    effects.extend(self.request_sessions());
                }
            }
            SessionAction::ToggleNamed => {
                if self.mode == AppMode::Sessions {
                    self.sessions.named_only = !self.sessions.named_only;
                    self.reset_overlay_selection();
                }
            }
            SessionAction::TogglePath => {
                if self.mode == AppMode::Sessions {
                    self.sessions.show_path = !self.sessions.show_path;
                }
            }
        }
        effects
    }

    fn dispatch_model_action(&mut self, action: ModelAction) -> Vec<Effect> {
        match action {
            ModelAction::RequestList => self.request_models(),
        }
    }

    fn dispatch_tree_action(&mut self, action: TreeAction) -> Vec<Effect> {
        match action {
            TreeAction::FoldOrUp => self.tree.fold_or_up_filtered(),
            TreeAction::UnfoldOrDown => self.tree.unfold_or_down_filtered(),
            TreeAction::EditLabel => {
                if !self.tree.begin_label_edit() {
                    self.status = "no tree entry selected".to_string();
                }
            }
            TreeAction::ToggleLabelTimestamp => {
                self.tree.show_label_timestamps = !self.tree.show_label_timestamps;
            }
            TreeAction::FilterCycleForward => self.cycle_tree_filter(1),
            TreeAction::FilterCycleBackward => self.cycle_tree_filter(4),
        }
        Vec::new()
    }

    fn dispatch_approval_action(&mut self, action: ApprovalAction) -> Vec<Effect> {
        match action {
            ApprovalAction::Respond(decision) => self.respond_approval(decision),
        }
    }

    fn dispatch_tool_interaction_action(&mut self, action: ToolInteractionAction) -> Vec<Effect> {
        match action {
            ToolInteractionAction::Submit => self.submit_tool_interaction(),
            ToolInteractionAction::Cancel => self.cancel_tool_interaction(),
            ToolInteractionAction::NextStep => {
                if let Some(interaction) = self.interactions.front_mut() {
                    if interaction.workflow.input_active() {
                        interaction.workflow.set_input_active(false);
                    } else {
                        interaction.workflow.next_step();
                    }
                }
                Vec::new()
            }
            ToolInteractionAction::PrevStep => {
                if let Some(interaction) = self.interactions.front_mut() {
                    if interaction.workflow.input_active() {
                        interaction.workflow.set_input_active(false);
                    } else {
                        interaction.workflow.prev_step();
                    }
                }
                Vec::new()
            }
            ToolInteractionAction::Choice(idx) => {
                if let Some(interaction) = self.interactions.front_mut() {
                    interaction.workflow.select_choice(idx);
                }
                Vec::new()
            }
        }
    }

    fn dispatch_notification_action(&mut self, action: NotificationAction) -> Vec<Effect> {
        match action {
            NotificationAction::Clear => self.notifications.clear(),
            NotificationAction::ClearAndClose => {
                self.notifications.clear();
                self.clear_focus();
                self.status = "notifications cleared".to_string();
            }
        }
        Vec::new()
    }

    fn dispatch_slash_action(&mut self, action: SlashAction) -> Vec<Effect> {
        let mut effects = Vec::new();
        match action {
            SlashAction::New => {
                effects.push(Effect::send(Command::SessionCreate {
                    command_id: command_id(),
                    cwd: self.cwd.to_string_lossy().into_owned(),
                }));
                self.clear_focus();
                self.status = "creating session".to_string();
            }
            SlashAction::Fork(entry_id) => effects.extend(self.fork_session(entry_id)),
            SlashAction::Clone => effects.extend(self.fork_session(None)),
            SlashAction::Rename(name) => effects.extend(self.rename_session(name)),
            SlashAction::Import(path) => {
                effects.push(Effect::send(Command::SessionImport {
                    command_id: command_id(),
                    path,
                }));
                self.status = "importing session".to_string();
            }
            SlashAction::Delete => effects.extend(self.delete_current_session()),
            SlashAction::Login(provider_opt) => {
                if let Some(provider) = provider_opt {
                    effects.push(Effect::send(Command::AuthLoginOAuth {
                        command_id: command_id(),
                        provider,
                    }));
                    self.status = "starting OAuth login".to_string();
                } else {
                    effects.push(Effect::send(Command::ModelList {
                        command_id: command_id(),
                    }));
                    let provider_names: Vec<String> = self
                        .model
                        .providers
                        .iter()
                        .map(|p| p.provider.clone())
                        .collect();
                    self.auth_selector.reset(&provider_names);
                    self.push_focus(AppMode::AuthSelector);
                    self.status = "Select authentication method".to_string();
                }
            }
            SlashAction::Logout(provider_opt) => {
                let provider = provider_opt
                    .or_else(|| self.model.active_provider.clone())
                    .unwrap_or_else(|| "anthropic".to_string());
                effects.push(Effect::send(Command::AuthLogout {
                    command_id: command_id(),
                    provider: provider.clone(),
                }));
                self.clear_focus();
                self.status = format!("logging out {provider}");
            }
            SlashAction::Compact => {
                let Some(session_id) = self.session.id.clone() else {
                    self.status = "no active session to compact".to_string();
                    return effects;
                };
                effects.push(Effect::send(Command::SessionCompact {
                    command_id: command_id(),
                    session_id,
                }));
                self.clear_focus();
                self.status = "compaction requested".to_string();
            }
        }
        effects
    }

    // ── surface selection helpers ─────────────────────────────────────────────

    pub fn has_suggestions(&self) -> bool {
        self.editor.auto_complete.is_active() && self.mode == AppMode::Chat
    }

    fn select_next(&mut self) {
        match self.mode {
            AppMode::Tree => self.tree.select_next_filtered(),
            AppMode::Settings => self.settings.select_next(),
            AppMode::Sessions => self.sessions.select_next(),
            AppMode::Models => self.models.select_next(),
            AppMode::AuthSelector => self.auth_selector.select_next(),
            _ => {}
        }
    }

    fn select_prev(&mut self) {
        match self.mode {
            AppMode::Tree => self.tree.select_prev_filtered(),
            AppMode::Settings => self.settings.select_prev(),
            AppMode::Sessions => self.sessions.select_prev(),
            AppMode::Models => self.models.select_prev(),
            AppMode::AuthSelector => self.auth_selector.select_prev(),
            _ => {}
        }
    }

    fn close_surface(&mut self) {
        match self.mode {
            AppMode::SummaryPrompt => {
                self.summary_prompt = None;
                self.pop_focus();
            }
            AppMode::Tree if self.tree.label_editor.is_some() => {
                self.tree.cancel_label_edit();
            }
            AppMode::ToolInteraction => {
                if let Some(interaction) = self.interactions.front_mut()
                    && interaction.workflow.input_active()
                {
                    interaction.workflow.set_input_active(false);
                    return;
                }
                self.pop_focus();
            }
            AppMode::Settings => {
                if !self.settings.pop() {
                    self.pop_focus();
                }
            }
            AppMode::Chat => {}
            _ => self.pop_focus(),
        }
    }

    fn select_surface_next(&mut self) {
        match self.mode {
            AppMode::SummaryPrompt => {
                if let Some(workflow) = self.summary_prompt.as_mut() {
                    workflow.next_step();
                }
            }
            _ => self.select_next(),
        }
    }

    fn select_surface_prev(&mut self) {
        match self.mode {
            AppMode::SummaryPrompt => {
                if let Some(workflow) = self.summary_prompt.as_mut() {
                    workflow.prev_step();
                }
            }
            _ => self.select_prev(),
        }
    }

    fn append_active_filter(&mut self, ch: char) {
        if let Some(text_box) = self.active_text_box() {
            text_box.insert_char(ch);
            return;
        }

        if let Some(filter) = self.active_filter_mut() {
            filter.push(ch);
        }

        match self.mode {
            AppMode::Tree => {
                self.tree.rebuild_visible_for_filter();
            }
            AppMode::Sessions => self.sessions.list.selected = 0,
            AppMode::Models => self.models.reset(),
            AppMode::Settings => self.settings.open_root(),
            AppMode::AuthSelector => {
                if let Some(frame) = self.auth_selector.menu.stack.last_mut() {
                    frame.list.selected = 0;
                }
            }
            _ => {}
        }
    }

    fn backspace_active_filter(&mut self) {
        if let Some(text_box) = self.active_text_box() {
            text_box.backspace();
            return;
        }

        if let Some(filter) = self.active_filter_mut() {
            filter.pop();
        }

        match self.mode {
            AppMode::Tree => {
                self.tree.rebuild_visible_for_filter();
            }
            AppMode::Sessions => self.sessions.list.selected = 0,
            AppMode::Models => self.models.reset(),
            AppMode::Settings => self.settings.open_root(),
            AppMode::AuthSelector => {
                if let Some(frame) = self.auth_selector.menu.stack.last_mut() {
                    frame.list.selected = 0;
                }
            }
            _ => {}
        }
    }

    fn cycle_tree_filter(&mut self, delta: u8) {
        use crate::features::tree::TreeFilterMode;

        let modes = [
            TreeFilterMode::Default,
            TreeFilterMode::NoTools,
            TreeFilterMode::UserOnly,
            TreeFilterMode::LabeledOnly,
            TreeFilterMode::All,
        ];
        let current = modes
            .iter()
            .position(|mode| *mode == self.tree.filter_mode)
            .unwrap_or(0);
        let next = (current + usize::from(delta)) % modes.len();
        self.tree.toggle_filter_for_current_search(modes[next]);
    }

    fn confirm_selection(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.focus_manager.active_mode() == AppMode::SummaryPrompt {
            let Some(workflow) = self.summary_prompt.as_mut() else {
                self.pop_focus();
                return effects;
            };

            let confirm = crate::features::tree::confirm_summary_prompt(workflow);
            match confirm {
                crate::features::tree::SummaryPromptConfirm::NeedsInput => return effects,
                crate::features::tree::SummaryPromptConfirm::Navigate {
                    entry_id,
                    summarize,
                    custom_instructions,
                } => {
                    self.summary_prompt = None;
                    self.pop_focus();
                    effects.extend(self.navigate_selected_tree_entry(
                        entry_id,
                        summarize,
                        custom_instructions,
                    ));
                }
                crate::features::tree::SummaryPromptConfirm::None => {
                    self.summary_prompt = None;
                    self.pop_focus();
                }
            }
            return effects;
        }

        if self.mode == AppMode::Tree && self.tree.label_editor.is_some() {
            if let Some(commit) = self.tree.take_label_edit_commit()
                && let Some(session_id) = &self.session.id
            {
                effects.push(Effect::send(piko_protocol::Command::SessionSetLabel {
                    command_id: command_id(),
                    session_id: session_id.clone(),
                    entry_id: commit.target_id,
                    label: commit.label,
                }));
            }
            return effects;
        }

        match self.mode {
            AppMode::Tree => {
                let Some(entry_id) = self.tree.selected_filtered_entry_id() else {
                    self.status = "no tree entry selected".to_string();
                    return effects;
                };
                if Some(&entry_id) == self.tree.document.current_leaf_id.as_ref() {
                    self.clear_focus();
                    self.status = "already at this point".to_string();
                    return effects;
                }

                if self.tree_navigation_needs_summary(&entry_id) && self.summary_prompt.is_none() {
                    self.summary_prompt = Some(crate::features::tree::create_summary_prompt(
                        entry_id.clone(),
                    ));
                    self.push_focus(AppMode::SummaryPrompt);
                    return effects;
                }

                self.summary_prompt = None;
                effects.extend(self.navigate_selected_tree_entry(entry_id, false, None));
            }
            AppMode::Sessions => effects.extend(self.open_selected_session()),
            AppMode::Models => effects.extend(self.apply_selected_model()),
            AppMode::Settings => effects.extend(self.apply_selected_setting()),
            AppMode::AuthSelector => {
                use crate::features::auth_selector::AuthConfirmResult;
                match self.auth_selector.confirm() {
                    AuthConfirmResult::StartOAuth { provider } => {
                        effects.push(Effect::send(piko_protocol::Command::AuthLoginOAuth {
                            command_id: command_id(),
                            provider: provider.clone(),
                        }));
                        self.status = format!("starting {provider} OAuth login");
                        self.pop_focus();
                    }
                    AuthConfirmResult::StartApiKeyInput => {}
                    AuthConfirmResult::SetApiKey { provider, api_key } => {
                        effects.push(Effect::send(piko_protocol::Command::AuthSetApiKey {
                            command_id: command_id(),
                            provider: provider.clone(),
                            api_key,
                        }));
                        self.status = format!("API key set for {provider}");
                        self.pop_focus();
                    }
                    AuthConfirmResult::None => {}
                }
            }
            AppMode::Status
            | AppMode::Help
            | AppMode::Chat
            | AppMode::Approval
            | AppMode::ToolInteraction => {}
            AppMode::SummaryPrompt => {}
        }
        effects
    }
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

    fn reset_overlay_selection(&mut self) {
        self.sessions.list.selected = 0;
        self.models.reset();
        self.settings.open_root();
        self.tree.selected_idx = 0;
    }
}
