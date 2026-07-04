use piko_protocol::{ApprovalDecision, Command, CommandCatalogAction, SessionTreeEntry};

use crate::{
    app::{AppMode, AppState, command::Action, command_id, config_command_for_setting},
    features::notifications::NotificationLevel,
    host::HostdClient,
    ui::components::hierarchical_menu::MenuConfirmResult,
};

impl AppState {
    // ── action dispatch ───────────────────────────────────────────────────────

    /// Main entry point for all intents.  Called from `main.rs` after
    /// translating raw key events or surface selections into `Action` values.
    pub fn dispatch(&mut self, host: &mut HostdClient, action: Action) {
        match action {
            Action::Quit => self.quit = true,

            // ── turn / chat ────────────────────────────────────────────────
            Action::Submit => self.submit(host),
            Action::Cancel => self.cancel(host),
            Action::CancelSuggestions => {
                self.editor.auto_complete.clear();
            }
            Action::InsertChar(ch) => {
                self.editor.insert_char(ch);
                self.refresh_suggestions();
            }
            Action::InsertPaste(text) => {
                if let Some(tb) = self.active_text_box() {
                    tb.insert_str(&text);
                } else {
                    self.editor.insert_paste(&text, &self.tui_config.editor);
                    self.refresh_suggestions();
                }
            }
            Action::InsertNewline => {
                self.editor.insert_newline();
                self.refresh_suggestions();
            }
            Action::DeleteBackward => {
                self.editor.backspace();
                self.refresh_suggestions();
            }
            Action::DeleteForward => {
                self.editor.delete();
                self.refresh_suggestions();
            }
            Action::CursorLeft => {
                self.editor.move_left();
                self.refresh_suggestions();
            }
            Action::CursorRight => {
                self.editor.move_right();
                self.refresh_suggestions();
            }
            Action::CursorLineStart => {
                self.editor.move_line_start();
                self.refresh_suggestions();
            }
            Action::CursorLineEnd => {
                self.editor.move_line_end();
                self.refresh_suggestions();
            }
            Action::HistoryPrev => self.history_prev(),
            Action::HistoryNext => self.history_next(),
            Action::AcceptSuggestion => self.accept_suggestion(),
            Action::AcceptAndSubmitSuggestion => {
                let keep_active = self
                    .editor
                    .auto_complete
                    .items
                    .get(self.editor.auto_complete.selected)
                    .is_some_and(|item| item.keep_active);
                self.accept_suggestion();
                if !keep_active {
                    self.submit(host);
                }
            }
            Action::SuggestionSelectNext => self.select_suggestion_next(),
            Action::SuggestionSelectPrev => self.select_suggestion_prev(),

            // ── timeline ──────────────────────────────────────────────────
            Action::TimelineScrollUp(n) => self.timeline.scroll_up(n),
            Action::TimelineScrollDown(n) => self.timeline.scroll_down(n),
            Action::TimelineJumpLatest => self.timeline.jump_latest(),

            // ── surface navigation ────────────────────────────────────────
            Action::OpenHelp => {
                self.push_focus(AppMode::Help);
                self.status = "help".to_string();
            }
            Action::OpenCommands => {
                self.focus_manager.clear_to_chat();
                self.mode = AppMode::Chat;
                let text = self.editor.text();
                if !text.starts_with('/') {
                    self.editor.insert_char('/');
                    self.refresh_suggestions();
                }
                self.status = "commands".to_string();
            }
            Action::OpenSettings => {
                self.settings.open_root();
                self.push_focus(AppMode::Settings);
                self.status = "settings".to_string();
            }
            Action::OpenThinking => {
                self.settings.open_thinking();
                self.push_focus(AppMode::Settings);
                self.status = "thinking level".to_string();
            }
            Action::OpenStatus => {
                self.push_focus(AppMode::Status);
                self.status = "status".to_string();
            }
            Action::OpenTree => {
                self.tree.filter_mode = self.tui_config.tree.filter_mode.into();
                self.push_focus(AppMode::Tree);
                self.tree.rebuild_visible(&self.filter_text);
                self.status = format!("{} session entries", self.tree.visible.rows.len());
            }
            Action::RequestSessions => self.request_sessions(host),
            Action::RequestModels => self.request_models(host),
            Action::CloseSurface => {
                if self.mode == AppMode::Settings && self.settings.pop() {
                    self.filter_text.clear();
                } else if self.mode == AppMode::AuthSelector {
                    use crate::features::auth_selector::AuthSelectorState;
                    match &mut self.auth_selector.state {
                        AuthSelectorState::ApiKeyInput { .. } => {
                            self.auth_selector.state = AuthSelectorState::Menu;
                            self.filter_text.clear();
                        }
                        AuthSelectorState::Menu => {
                            if self.auth_selector.menu.pop() {
                                self.filter_text.clear();
                            } else {
                                self.pop_focus();
                            }
                        }
                    }
                } else if self.focus_manager.active_mode() == AppMode::SummaryPrompt {
                    self.summary_prompt = None;
                    self.pop_focus();
                } else if self.mode == AppMode::Tree && self.tree.label_editor.is_some() {
                    self.tree.label_editor = None;
                } else if !self.filter_text.is_empty() {
                    self.filter_text.clear();
                    if self.mode == AppMode::Tree {
                        self.tree.rebuild_visible("");
                    }
                    self.reset_overlay_selection();
                } else {
                    self.pop_focus();
                }
            }

            Action::TreeFoldOrUp => self.tree.fold_or_up(&self.filter_text),
            Action::TreeUnfoldOrDown => self.tree.unfold_or_down(&self.filter_text),
            Action::TreeEditLabel => {
                let Some(id) = self.tree.selected_entry_id(&self.filter_text) else {
                    self.status = "no tree entry selected".to_string();
                    return;
                };
                self.tree.label_editor = Some(crate::features::tree::LabelEditorState {
                    target_id: id,
                    input: crate::ui::components::text_box::TextBox::new(),
                });
            }
            Action::TreeToggleLabelTimestamp => {
                self.tree.show_label_timestamps = !self.tree.show_label_timestamps;
            }
            Action::TreeFilterCycleForward => {
                let current = self.tree.filter_mode as u8;
                let next = (current + 1) % 5;
                let mode = match next {
                    0 => crate::features::tree::TreeFilterMode::Default,
                    1 => crate::features::tree::TreeFilterMode::NoTools,
                    2 => crate::features::tree::TreeFilterMode::UserOnly,
                    3 => crate::features::tree::TreeFilterMode::LabeledOnly,
                    4 => crate::features::tree::TreeFilterMode::All,
                    _ => unreachable!(),
                };
                self.tree.toggle_filter(mode, &self.filter_text);
            }
            Action::TreeFilterCycleBackward => {
                let current = self.tree.filter_mode as u8;
                let next = (current + 4) % 5; // -1 mod 5
                let mode = match next {
                    0 => crate::features::tree::TreeFilterMode::Default,
                    1 => crate::features::tree::TreeFilterMode::NoTools,
                    2 => crate::features::tree::TreeFilterMode::UserOnly,
                    3 => crate::features::tree::TreeFilterMode::LabeledOnly,
                    4 => crate::features::tree::TreeFilterMode::All,
                    _ => unreachable!(),
                };
                self.tree.toggle_filter(mode, &self.filter_text);
            }

            // ── list selection ────────────────────────────────────────────
            Action::SelectNext => {
                if self.focus_manager.active_mode() == AppMode::SummaryPrompt {
                    if let Some(state) = &mut self.summary_prompt {
                        state.select_next();
                    }
                } else {
                    self.select_next();
                }
            }
            Action::SelectPrev => {
                if self.focus_manager.active_mode() == AppMode::SummaryPrompt {
                    if let Some(state) = &mut self.summary_prompt {
                        state.select_prev();
                    }
                } else {
                    self.select_prev();
                }
            }
            Action::ConfirmSelection => self.confirm_selection(host),
            Action::FilterAppend(ch) => {
                if let Some(tb) = self.active_text_box() {
                    tb.insert_char(ch);
                } else {
                    self.filter_text.push(ch);
                    if self.mode == AppMode::Tree {
                        self.tree.folded.clear();
                        self.tree.rebuild_visible(&self.filter_text);
                    }
                    self.reset_overlay_selection();
                }
            }
            Action::FilterBackspace => {
                if let Some(tb) = self.active_text_box() {
                    tb.backspace();
                } else {
                    self.filter_text.pop();
                    if self.mode == AppMode::Tree {
                        self.tree.folded.clear();
                        self.tree.rebuild_visible(&self.filter_text);
                    }
                    self.reset_overlay_selection();
                }
            }
            Action::SessionToggleScope => {
                if self.mode == AppMode::Sessions {
                    self.sessions.scope = match self.sessions.scope {
                        crate::features::session_list::SessionScope::CurrentFolder => {
                            crate::features::session_list::SessionScope::All
                        }
                        crate::features::session_list::SessionScope::All => {
                            crate::features::session_list::SessionScope::CurrentFolder
                        }
                    };
                    self.request_sessions(host);
                }
            }
            Action::SessionToggleNamed => {
                if self.mode == AppMode::Sessions {
                    self.sessions.named_only = !self.sessions.named_only;
                    self.reset_overlay_selection();
                }
            }
            Action::SessionTogglePath => {
                if self.mode == AppMode::Sessions {
                    self.sessions.show_path = !self.sessions.show_path;
                }
            }
            // ── approval ──────────────────────────────────────────────────
            Action::ApprovalRespond(decision) => self.respond_approval(host, decision),

            // ── tool interaction ──────────────────────────────────────────
            Action::ToolInteractionSubmit => self.submit_tool_interaction(host),
            Action::ToolInteractionCancel => self.cancel_tool_interaction(host),
            Action::ToolInteractionNextStep => {
                if let Some(interaction) = self.interactions.front_mut() {
                    if interaction.workflow.input_active() {
                        interaction.workflow.set_input_active(false);
                    } else {
                        interaction.workflow.next_step();
                    }
                }
            }
            Action::ToolInteractionPrevStep => {
                if let Some(interaction) = self.interactions.front_mut() {
                    if interaction.workflow.input_active() {
                        interaction.workflow.set_input_active(false);
                    } else {
                        interaction.workflow.prev_step();
                    }
                }
            }
            Action::ToolInteractionChoice(idx) => {
                if let Some(interaction) = self.interactions.front_mut() {
                    interaction.workflow.select_choice(idx);
                }
            }

            // ── notifications ─────────────────────────────────────────────
            Action::ClearNotifications => self.notifications.clear(),

            // ── slash commands ────────────────────────────────────────────
            Action::SlashNew => {
                match host.send(Command::SessionCreate {
                    command_id: command_id(),
                    cwd: self.cwd.to_string_lossy().into_owned(),
                }) {
                    Ok(()) => self.status = "creating session".to_string(),
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            Action::SlashFork(entry_id) => self.fork_session(host, entry_id),
            Action::SlashClone => self.fork_session(host, None),
            Action::SlashRename(name) => self.rename_session(host, name),
            Action::SlashImport(path) => {
                match host.send(Command::SessionImport {
                    command_id: command_id(),
                    path,
                }) {
                    Ok(()) => self.status = "importing session".to_string(),
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            Action::SlashDelete => self.delete_current_session(host),
            Action::SlashLogin(provider_opt) => {
                if let Some(provider) = provider_opt {
                    match host.send(Command::AuthLoginOAuth {
                        command_id: command_id(),
                        provider,
                    }) {
                        Ok(()) => self.status = "starting OAuth login".to_string(),
                        Err(err) => self.push_error(err.to_string()),
                    }
                } else {
                    let _ = host.send(Command::ModelList {
                        command_id: command_id(),
                    });
                    let provider_names: Vec<String> =
                        self.providers.iter().map(|p| p.provider.clone()).collect();
                    self.auth_selector.reset(&provider_names);
                    self.push_focus(AppMode::AuthSelector);
                    self.status = "Select authentication method".to_string();
                }
            }
            Action::SlashLogout(provider_opt) => {
                let provider = provider_opt
                    .or_else(|| self.active_provider.clone())
                    .unwrap_or_else(|| "anthropic".to_string());
                match host.send(Command::AuthLogout {
                    command_id: command_id(),
                    provider,
                }) {
                    Ok(()) => self.status = "logging out".to_string(),
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            Action::SlashCompact => {
                let Some(session_id) = self.session_id.clone() else {
                    self.status = "no active session to compact".to_string();
                    return;
                };
                match host.send(Command::SessionCompact {
                    command_id: command_id(),
                    session_id,
                }) {
                    Ok(()) => self.status = "compaction requested".to_string(),
                    Err(err) => self.push_error(err.to_string()),
                }
            }
        }
    }

    // ── surface selection helpers ─────────────────────────────────────────────

    pub fn has_suggestions(&self) -> bool {
        self.editor.auto_complete.is_active() && self.mode == AppMode::Chat
    }

    fn select_next(&mut self) {
        match self.mode {
            AppMode::Tree => self.tree.select_next(&self.filter_text),
            AppMode::Settings => self.settings.select_next(&self.filter_text),
            AppMode::Sessions => self.sessions.select_next(&self.filter_text),
            AppMode::Models => self.models.select_next(&self.filter_text),
            AppMode::AuthSelector => self.auth_selector.select_next(&self.filter_text),
            _ => {}
        }
    }

    fn select_prev(&mut self) {
        match self.mode {
            AppMode::Tree => self.tree.select_prev(&self.filter_text),
            AppMode::Settings => self.settings.select_prev(&self.filter_text),
            AppMode::Sessions => self.sessions.select_prev(&self.filter_text),
            AppMode::Models => self.models.select_prev(&self.filter_text),
            AppMode::AuthSelector => self.auth_selector.select_prev(&self.filter_text),
            _ => {}
        }
    }

    fn confirm_selection(&mut self, host: &mut HostdClient) {
        if self.focus_manager.active_mode() == AppMode::SummaryPrompt {
            let Some(state) = self.summary_prompt.as_mut() else {
                self.pop_focus();
                return;
            };

            if state.questions.is_empty() {
                self.pop_focus();
                return;
            }

            let active_q = &mut state.questions[state.active_question_idx];
            let choice = &active_q.choices[active_q.selected_idx];

            if choice.has_input && !active_q.is_input_active {
                active_q.is_input_active = true;
                return;
            }

            let mut should_summarize = false;
            let mut custom_instructions = None;

            // index 0 -> NoSummary
            // index 1 -> DefaultSummary
            // index 2 -> CustomInstructions
            match active_q.selected_idx {
                0 => {}
                1 => should_summarize = true,
                2 => {
                    should_summarize = true;
                    custom_instructions = Some(active_q.input_value.text().to_string());
                }
                _ => {}
            }

            let entry_id = state.target_entry_id.clone().unwrap_or_default();
            self.summary_prompt = None;
            self.pop_focus();
            self.navigate_selected_tree_entry(
                host,
                entry_id,
                should_summarize,
                custom_instructions,
            );
            return;
        }

        if self.mode == AppMode::Tree && self.tree.label_editor.is_some() {
            if let Some(state) = self.tree.label_editor.take()
                && let Some(session_id) = &self.session_id
                && let Err(err) = host.send(piko_protocol::Command::SessionSetLabel {
                    command_id: command_id(),
                    session_id: session_id.clone(),
                    entry_id: state.target_id,
                    label: if state.input.text().trim().is_empty() {
                        None
                    } else {
                        Some(state.input.text().to_string())
                    },
                })
            {
                self.push_error(err.to_string());
            }
            return;
        }

        match self.mode {
            AppMode::Tree => {
                let Some(entry_id) = self.tree.selected_entry_id(&self.filter_text) else {
                    self.status = "no tree entry selected".to_string();
                    return;
                };
                if Some(&entry_id) == self.tree.document.current_leaf_id.as_ref() {
                    self.clear_focus();
                    self.status = "already at this point".to_string();
                    return;
                }

                if self.tree_navigation_needs_summary(&entry_id) && self.summary_prompt.is_none() {
                    self.summary_prompt = Some(crate::features::tree::create_summary_prompt(
                        entry_id.clone(),
                    ));
                    self.push_focus(AppMode::SummaryPrompt);
                    return;
                }

                self.summary_prompt = None;
                self.navigate_selected_tree_entry(host, entry_id, false, None);
            }
            AppMode::Sessions => self.open_selected_session(host),
            AppMode::Models => self.apply_selected_model(host),
            AppMode::Settings => self.apply_selected_setting(host),
            AppMode::AuthSelector => {
                use crate::features::auth_selector::{AuthAction, AuthSelectorState};
                use crate::ui::components::hierarchical_menu::MenuConfirmResult;
                match &mut self.auth_selector.state {
                    AuthSelectorState::Menu => {
                        let confirm_res = self.auth_selector.menu.confirm(&mut self.filter_text);
                        if let MenuConfirmResult::Action(action, _) = confirm_res {
                            match action {
                                AuthAction::StartOAuth { provider } => {
                                    match host.send(piko_protocol::Command::AuthLoginOAuth {
                                        command_id: command_id(),
                                        provider: provider.clone(),
                                    }) {
                                        Ok(()) => {
                                            self.status = format!("starting {provider} OAuth login")
                                        }
                                        Err(err) => self.push_error(err.to_string()),
                                    }
                                    self.pop_focus();
                                }
                                AuthAction::StartApiKey { provider } => {
                                    self.auth_selector.state = AuthSelectorState::ApiKeyInput {
                                        provider,
                                        input: crate::ui::components::text_box::TextBox::new()
                                            .with_mask('•')
                                            .with_placeholder("Paste API key here..."),
                                    };
                                    self.filter_text.clear();
                                }
                            }
                        }
                    }
                    AuthSelectorState::ApiKeyInput { provider, input } => {
                        match host.send(piko_protocol::Command::AuthSetApiKey {
                            command_id: command_id(),
                            provider: provider.clone(),
                            api_key: input.text().to_string(),
                        }) {
                            Ok(()) => self.status = format!("API key set for {provider}"),
                            Err(err) => self.push_error(err.to_string()),
                        }
                        self.pop_focus();
                    }
                }
            }
            AppMode::Status
            | AppMode::Help
            | AppMode::Chat
            | AppMode::Approval
            | AppMode::ToolInteraction => {}
            AppMode::SummaryPrompt => {}
        }
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

    // ── input helpers ─────────────────────────────────────────────────────────

    fn submit(&mut self, host: &mut HostdClient) {
        let submitted_draft = self.editor.text();
        let Some(text) = self.editor.take_trimmed() else {
            return;
        };
        self.refresh_suggestions();
        if text.starts_with('/') {
            if self.try_slash_command(host, &text) {
                return;
            } else {
                self.editor.restore_text(&submitted_draft);
                self.status = format!("Unknown slash command: {}", text);
                self.notify(
                    NotificationLevel::Error,
                    format!("Unknown slash command: {}", text),
                );
                return;
            }
        }
        let Some(session_id) = self.session_id.clone() else {
            // Buffer the submitted text
            self.pending_turn_text = Some(text);

            // Only create a session if we aren't already waiting for an initial session to open
            if !self.session_initializing {
                self.session_initializing = true;
                let _ = host.send(Command::SessionCreate {
                    command_id: command_id(),
                    cwd: self.cwd.to_string_lossy().into_owned(),
                });
                self.status = "creating session...".to_string();
            } else {
                self.status = "waiting for session...".to_string();
            }
            return;
        };
        match host.send(Command::TurnSubmit {
            command_id: command_id(),
            session_id,
            text,
        }) {
            Ok(()) => {
                self.status = "submitted turn".to_string();
                self.notify(NotificationLevel::Info, "submitted turn");
            }
            Err(err) => self.push_error(err.to_string()),
        }
    }

    fn cancel(&mut self, host: &mut HostdClient) {
        let (Some(session_id), Some(turn_id)) =
            (self.session_id.clone(), self.active_turn_id.clone())
        else {
            self.editor.restore_text("");
            self.status = "editor cleared".to_string();
            return;
        };
        match host.send(Command::TurnCancel {
            command_id: command_id(),
            session_id,
            turn_id,
        }) {
            Ok(()) => self.status = "cancel requested".to_string(),
            Err(err) => self.push_error(err.to_string()),
        }
    }

    fn respond_approval(&mut self, host: &mut HostdClient, decision: ApprovalDecision) {
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        let Some(approval) = self.approvals.front() else {
            self.status = "no pending approval".to_string();
            return;
        };
        let decision_label = format!("{decision:?}");
        let approval_id = approval.id.clone();
        match host.send(Command::ApprovalRespond {
            command_id: command_id(),
            session_id,
            approval_id,
            decision,
            note: None,
        }) {
            Ok(()) => {
                self.status = format!("approval response sent: {decision_label}");
                self.notify(
                    NotificationLevel::Info,
                    format!("approval response sent: {decision_label}"),
                );
            }
            Err(err) => self.push_error(err.to_string()),
        }
    }

    fn submit_tool_interaction(&mut self, host: &mut HostdClient) {
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        let Some(interaction) = self.interactions.front_mut() else {
            self.status = "no pending interaction".to_string();
            return;
        };
        let workflow = &mut interaction.workflow;
        if workflow.input_active() {
            workflow.set_input_active(false);
            if !workflow.require_confirm && workflow.questions.len() == 1 {
                // fall through to submit below
            } else {
                return;
            }
        } else if !workflow.confirm_focused {
            let question = &workflow.questions[workflow.active_question_idx];
            if question
                .choices
                .get(question.selected_idx)
                .is_some_and(|choice| choice.has_input)
            {
                workflow.set_input_active(true);
                return;
            }
            if workflow.require_confirm
                || workflow.active_question_idx + 1 < workflow.questions.len()
            {
                workflow.next_step();
                return;
            }
        }

        if let Some((interaction_id, response)) = self.interactions.submit_response() {
            match host.send(Command::UserInteractionRespond {
                command_id: command_id(),
                session_id,
                interaction_id,
                response,
            }) {
                Ok(()) => self.status = "interaction response sent".to_string(),
                Err(err) => self.push_error(err.to_string()),
            }
        }
    }

    fn cancel_tool_interaction(&mut self, host: &mut HostdClient) {
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        if let Some(interaction) = self.interactions.front_mut()
            && interaction.workflow.input_active()
        {
            interaction.workflow.set_input_active(false);
            return;
        }
        let Some((interaction_id, response)) = self.interactions.cancel_response() else {
            self.status = "no pending interaction".to_string();
            return;
        };
        match host.send(Command::UserInteractionRespond {
            command_id: command_id(),
            session_id,
            interaction_id,
            response,
        }) {
            Ok(()) => self.status = "interaction cancelled".to_string(),
            Err(err) => self.push_error(err.to_string()),
        }
    }

    pub fn refresh_suggestions(&mut self) {
        let text = self.editor.text();
        self.editor.auto_complete.update(
            &self.cwd,
            &self.command_catalog,
            &text,
            self.editor.cursor(),
        );
    }

    fn select_suggestion_next(&mut self) {
        self.editor.auto_complete.select_next();
        self.fill_editor_with_selected_suggestion();
    }

    fn select_suggestion_prev(&mut self) {
        self.editor.auto_complete.select_prev();
        self.fill_editor_with_selected_suggestion();
    }

    fn accept_suggestion(&mut self) {
        if let Some(completion) = self.editor.auto_complete.accept() {
            let cursor = self.editor.cursor();
            let is_file = completion.replacement.starts_with('@');
            if is_file {
                self.editor.replace_range(completion.start, cursor, "");
                let trimmed = completion.replacement.trim_end().to_string();
                let placeholder = format!("[{trimmed}]");
                self.editor.insert_reference_block(placeholder, trimmed);
                if completion.replacement.ends_with(' ') {
                    self.editor.insert_char(' ');
                }
            } else {
                self.editor
                    .replace_range(completion.start, cursor, &completion.replacement);
            }
            self.refresh_suggestions();
        }
    }

    fn fill_editor_with_selected_suggestion(&mut self) {
        let selected_idx = self.editor.auto_complete.selected;
        if let Some(item) = self.editor.auto_complete.items.get(selected_idx).cloned() {
            let cursor = self.editor.cursor();
            let is_file = item.replacement.starts_with('@');
            if is_file {
                self.editor.replace_range(item.start, cursor, "");
                let trimmed = item.replacement.trim_end().to_string();
                let placeholder = format!("[{trimmed}]");
                self.editor.insert_reference_block(placeholder, trimmed);
                if item.replacement.ends_with(' ') {
                    self.editor.insert_char(' ');
                }
            } else {
                self.editor
                    .replace_range(item.start, cursor, &item.replacement);
            }
        }
    }

    fn history_prev(&mut self) {
        self.editor.history_prev();
        self.refresh_suggestions();
    }

    fn history_next(&mut self) {
        self.editor.history_next();
        self.refresh_suggestions();
    }

    // ── session operations ────────────────────────────────────────────────────

    fn request_sessions(&mut self, host: &mut HostdClient) {
        self.sessions.loading = true;
        let scope = self.sessions.scope.to_protocol();
        let cwd = Some(self.cwd.to_string_lossy().into_owned());
        let command_id = command_id();
        match host.send(Command::SessionList {
            command_id: command_id.clone(),
            scope,
            cwd,
        }) {
            Ok(()) => {
                self.pending_session_list_command_id = Some(command_id);
                self.push_focus(AppMode::Sessions);
                self.status = "loading sessions".to_string();
            }
            Err(err) => {
                self.sessions.loading = false;
                self.sessions.error = Some(err.to_string());
                self.push_error(err.to_string());
            }
        }
    }

    fn request_models(&mut self, host: &mut HostdClient) {
        match host.send(Command::ModelList {
            command_id: command_id(),
        }) {
            Ok(()) => {
                self.push_focus(AppMode::Models);
                self.status = "loading models".to_string();
            }
            Err(err) => self.push_error(err.to_string()),
        }
    }

    fn open_selected_session(&mut self, host: &mut HostdClient) {
        let Some(summary) = self.sessions.selected_session_summary(&self.filter_text) else {
            self.status = "no session selected".to_string();
            return;
        };
        self.sessions.loading = true;
        let command_id = command_id();
        match host.send(Command::SessionOpen {
            command_id: command_id.clone(),
            session_id: summary.session_id,
            session_path: summary.session_path,
        }) {
            Ok(()) => {
                self.pending_session_open_command_id = Some(command_id);
                self.status = "opening session".to_string();
            }
            Err(err) => {
                self.sessions.loading = false;
                self.sessions.error = Some(err.to_string());
                self.push_error(err.to_string());
            }
        }
    }

    fn navigate_selected_tree_entry(
        &mut self,
        host: &mut HostdClient,
        entry_id: String,
        summarize: bool,
        custom_instructions: Option<String>,
    ) {
        let Some(session_id) = self.session_id.clone() else {
            self.status = "no active session".to_string();
            return;
        };
        match host.send(piko_protocol::Command::SessionNavigate {
            command_id: command_id(),
            session_id,
            entry_id,
            summarize,
            custom_instructions,
        }) {
            Ok(()) => {
                self.clear_focus();
                self.status = "navigating session tree".to_string();
            }
            Err(err) => self.push_error(err.to_string()),
        }
    }

    fn apply_selected_model(&mut self, host: &mut HostdClient) {
        match self.models.confirm(&mut self.filter_text) {
            MenuConfirmResult::Action(model, _) => {
                let provider = model.provider.clone();
                let model_id = model.id.clone();
                match host.send(Command::ConfigUpdate {
                    command_id: command_id(),
                    patch: serde_json::json!({
                        "default-provider": provider,
                        "default-model": model_id,
                    }),
                }) {
                    Ok(()) => {
                        self.clear_focus();
                        self.status = format!("switching model to {provider}/{model_id}");
                    }
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            _ => {
                self.status = "no model selected".to_string();
            }
        }
    }

    fn apply_selected_setting(&mut self, host: &mut HostdClient) {
        use crate::features::settings::SettingsConfirmResult;
        match self.settings.confirm(&mut self.filter_text) {
            SettingsConfirmResult::SubMenuPushed => {}
            SettingsConfirmResult::Action(action, title) => {
                let command = config_command_for_setting(action);
                match host.send(command) {
                    Ok(()) => {
                        self.clear_focus();
                        self.status = format!("setting applied: {}", title);
                        self.notify(NotificationLevel::Info, self.status.clone());
                    }
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            SettingsConfirmResult::None => {
                self.status = "no setting selected".to_string();
            }
        }
    }

    fn fork_session(&mut self, host: &mut HostdClient, entry_id: Option<String>) {
        let Some(session_id) = self.session_id.clone() else {
            self.status = "no active session to fork".to_string();
            return;
        };
        match host.send(Command::SessionFork {
            command_id: command_id(),
            session_id,
            entry_id,
        }) {
            Ok(()) => {
                self.clear_focus();
                self.status = "forking session".to_string();
            }
            Err(err) => self.push_error(err.to_string()),
        }
    }

    fn rename_session(&mut self, host: &mut HostdClient, name: String) {
        let Some(session_id) = self.session_id.clone() else {
            self.status = "no active session to rename".to_string();
            return;
        };
        match host.send(Command::SessionRename {
            command_id: command_id(),
            session_id,
            name: name.clone(),
        }) {
            Ok(()) => {
                self.status = format!("renaming session to {name}");
                self.notify(NotificationLevel::Info, self.status.clone());
            }
            Err(err) => self.push_error(err.to_string()),
        }
    }

    fn delete_current_session(&mut self, host: &mut HostdClient) {
        let Some(session_id) = self.session_id.clone() else {
            self.status = "no active session to delete".to_string();
            return;
        };
        match host.send(Command::SessionDelete {
            command_id: command_id(),
            session_id,
        }) {
            Ok(()) => {
                self.session_id = None;
                self.timeline.clear();
                self.tree.document = Default::default();
                self.tree.visible.rows.clear();
                self.clear_focus();
                self.status = "session deleted".to_string();
                self.notify(NotificationLevel::Warning, "session deleted");
            }
            Err(err) => self.push_error(err.to_string()),
        }
    }

    // ── command palette dispatch ──────────────────────────────────────────────

    pub fn run_command_action(&mut self, host: &mut HostdClient, action: CommandCatalogAction) {
        match action {
            CommandCatalogAction::Help => {
                self.push_focus(AppMode::Help);
                self.status = "help".to_string();
            }
            CommandCatalogAction::Commands => {
                self.focus_manager.clear_to_chat();
                self.mode = AppMode::Chat;
                let text = self.editor.text();
                if !text.starts_with('/') {
                    self.editor.insert_char('/');
                    self.refresh_suggestions();
                }
                self.status = "commands".to_string();
            }
            CommandCatalogAction::Sessions => self.request_sessions(host),
            CommandCatalogAction::Models => self.request_models(host),
            CommandCatalogAction::Thinking => {
                self.settings.open_thinking();
                self.push_focus(AppMode::Settings);
                self.status = "thinking level".to_string();
            }
            CommandCatalogAction::Tree => {
                self.tree.filter_mode = self.tui_config.tree.filter_mode.into();
                self.push_focus(AppMode::Tree);
                self.tree.rebuild_visible(&self.filter_text);
                self.status = format!("{} session entries", self.tree.visible.rows.len());
            }
            CommandCatalogAction::Settings => {
                self.settings.open_root();
                self.push_focus(AppMode::Settings);
                self.status = "settings".to_string();
            }
            CommandCatalogAction::Status => {
                self.push_focus(AppMode::Status);
                self.status = "status".to_string();
            }
            CommandCatalogAction::NewSession => {
                match host.send(Command::SessionCreate {
                    command_id: command_id(),
                    cwd: self.cwd.to_string_lossy().into_owned(),
                }) {
                    Ok(()) => {
                        self.clear_focus();
                        self.status = "creating session".to_string();
                    }
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            CommandCatalogAction::ForkSession => {
                let entry_id = self.tree.selected_entry_id(&self.filter_text);
                self.fork_session(host, entry_id);
            }
            CommandCatalogAction::CloneSession => self.fork_session(host, None),
            CommandCatalogAction::RenameSession
            | CommandCatalogAction::ImportSession
            | CommandCatalogAction::ExportSession
            | CommandCatalogAction::DeleteSession => {
                self.status = "this command requires slash arguments".to_string();
            }
            CommandCatalogAction::Login => {
                let _ = host.send(Command::ModelList {
                    command_id: command_id(),
                });
                let provider_names: Vec<String> =
                    self.providers.iter().map(|p| p.provider.clone()).collect();
                self.auth_selector.reset(&provider_names);
                self.push_focus(AppMode::AuthSelector);
                self.status = "Select authentication method".to_string();
            }
            CommandCatalogAction::Logout => {
                let provider = self
                    .active_provider
                    .clone()
                    .unwrap_or_else(|| "anthropic".to_string());
                match host.send(Command::AuthLogout {
                    command_id: command_id(),
                    provider: provider.clone(),
                }) {
                    Ok(()) => {
                        self.clear_focus();
                        self.status = format!("logging out {provider}");
                    }
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            CommandCatalogAction::Compact => {
                let Some(session_id) = self.session_id.clone() else {
                    self.status = "no active session to compact".to_string();
                    return;
                };
                match host.send(Command::SessionCompact {
                    command_id: command_id(),
                    session_id,
                }) {
                    Ok(()) => {
                        self.clear_focus();
                        self.status = "compaction requested".to_string();
                    }
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            CommandCatalogAction::SetThinking { level } => {
                match host.send(Command::ConfigUpdate {
                    command_id: command_id(),
                    patch: serde_json::json!({
                        "default-thinking-level": level
                    }),
                }) {
                    Ok(()) => {
                        self.clear_focus();
                        self.status = format!("thinking level {level}");
                    }
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            CommandCatalogAction::ToggleToolsExpanded => {
                self.timeline.tools_expanded = !self.timeline.tools_expanded;
                self.clear_focus();
                self.status = if self.timeline.tools_expanded {
                    "tool details expanded".to_string()
                } else {
                    "tool details folded".to_string()
                };
                self.notify(NotificationLevel::Info, self.status.clone());
            }
            CommandCatalogAction::ClearNotifications => {
                self.notifications.clear();
                self.clear_focus();
                self.status = "notifications cleared".to_string();
            }
            CommandCatalogAction::Quit => self.quit = true,
        }
    }
}
