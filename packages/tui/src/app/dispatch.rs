use piko_protocol::Command;

use crate::app::{
    AppMode, AppState,
    command::{
        Action, AgentAction, AppAction, ApprovalAction, EditorAction, ModelAction,
        NotificationAction, SessionAction, SlashAction, SurfaceAction, TimelineAction,
        ToolInteractionAction, TreeAction,
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
            Action::AgentList(action) => self.dispatch_agent_list_action(action),
            Action::Tree(action) => self.dispatch_tree_action(action),
            Action::Approval(action) => self.dispatch_approval_action(action),
            Action::ToolInteraction(action) => self.dispatch_tool_interaction_action(action),
            Action::Notifications(action) => self.dispatch_notification_action(action),
            Action::Slash(action) => self.dispatch_slash_action(action),
            Action::AgentPanel(action) => self.dispatch_agent_panel_action(action),
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
            TimelineAction::ToggleToolsExpanded => {
                self.timeline.tools_expanded = !self.timeline.tools_expanded;
                self.clear_focus();
                self.status = if self.timeline.tools_expanded {
                    "tool details expanded".to_string()
                } else {
                    "tool details folded".to_string()
                };
                self.notify(
                    crate::features::notifications::NotificationLevel::Info,
                    self.status.clone(),
                );
            }
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

    fn dispatch_agent_list_action(&mut self, action: AgentAction) -> Vec<Effect> {
        match action {
            AgentAction::RequestList => {
                self.agents.loading = true;
                self.push_focus(AppMode::AgentList);
                vec![Effect::send(Command::AgentSpecList {
                    command_id: command_id(),
                })]
            }
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
                self.begin_session_hydration(None);
                let create_id = command_id();
                self.session.pending.track(
                    create_id.clone(),
                    super::pending::PendingCommandKind::SessionCreate,
                );
                effects.push(Effect::send(Command::SessionCreate {
                    command_id: create_id,
                    cwd: self.cwd.to_string_lossy().into_owned(),
                }));
                self.clear_focus();
                self.status = "creating session".to_string();
            }
            SlashAction::Fork(entry_id) => effects.extend(self.fork_session(entry_id)),
            SlashAction::Clone => effects.extend(self.fork_session(None)),
            SlashAction::Rename(name) => effects.extend(self.rename_session(name)),
            SlashAction::Import(path) => {
                self.begin_session_hydration(None);
                let import_id = command_id();
                self.session.pending.track(
                    import_id.clone(),
                    super::pending::PendingCommandKind::SessionOpen,
                );
                effects.push(Effect::send(Command::SessionImport {
                    command_id: import_id,
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
                let Some(agent_instance_id) = self.agent_panel.active_agent_instance_id.clone()
                else {
                    self.status = "no active agent to compact".to_string();
                    return effects;
                };
                effects.push(Effect::send(Command::SessionCompact {
                    command_id: command_id(),
                    session_id,
                    agent_instance_id,
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
            AppMode::AgentList => self.agents.move_down(),
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
            AppMode::AgentList => self.agents.move_up(),
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
            AppMode::AgentList => self.agents.list.selected = 0,
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
            AppMode::AgentList => self.agents.list.selected = 0,
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
        if self.focus_manager.active_mode() == AppMode::SummaryPrompt {
            return self.confirm_summary_prompt();
        }

        if self.mode == AppMode::Tree && self.tree.label_editor.is_some() {
            return self.confirm_tree_label_edit();
        }

        match self.mode {
            AppMode::Tree => self.confirm_tree_entry(),
            AppMode::Sessions => self.open_selected_session(),
            AppMode::AgentList => {
                self.pop_focus(); // Just close the view
                Vec::new()
            }
            AppMode::Models => self.apply_selected_model(),
            AppMode::Settings => self.apply_selected_setting(),
            AppMode::AuthSelector => self.confirm_auth_selection(),
            AppMode::Status
            | AppMode::Help
            | AppMode::Chat
            | AppMode::Approval
            | AppMode::ToolInteraction
            | AppMode::SummaryPrompt
            | AppMode::AgentPanel => Vec::new(),
        }
    }

    fn reset_overlay_selection(&mut self) {
        self.sessions.list.selected = 0;
        self.models.reset();
        self.settings.open_root();
        self.tree.selected_idx = 0;
    }

    fn dispatch_agent_panel_action(
        &mut self,
        action: super::command::AgentPanelAction,
    ) -> Vec<Effect> {
        match action {
            super::command::AgentPanelAction::Subscribe {
                agent_instance_id,
                agent_id,
            } => {
                let session_id = match self.session.id.clone() {
                    Some(id) => id,
                    None => return vec![],
                };
                self.status = format!("switching to agent {agent_id}");
                vec![Effect::send(Command::AgentSubscribe {
                    command_id: command_id(),
                    session_id,
                    agent_instance_id,
                    after_seq: None,
                })]
            }
        }
    }
}
