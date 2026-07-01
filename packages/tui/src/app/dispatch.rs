use piko_protocol::{ApprovalDecision, Command, CommandCatalogAction};

use crate::{
    app::{
        AppMode, AppState, command::Action, command_id, config_command_for_setting,
        empty_config_set_with,
    },
    features::notifications::NotificationLevel,
    host::HostdClient,
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
                self.editor.insert_paste(&text, &self.tui_config.editor);
                self.refresh_suggestions();
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
                self.push_focus(AppMode::Settings);
                self.status = "settings".to_string();
            }
            Action::OpenStatus => {
                self.push_focus(AppMode::Status);
                self.status = "status".to_string();
            }
            Action::OpenTree => {
                self.push_focus(AppMode::Tree);
                self.status = format!("{} session entries", self.tree.list.items.len());
            }
            Action::RequestSessions => self.request_sessions(host),
            Action::RequestModels => self.request_models(host),
            Action::CloseSurface => self.pop_focus(),

            // ── list selection ────────────────────────────────────────────
            Action::SelectNext => self.select_next(),
            Action::SelectPrev => self.select_prev(),
            Action::ConfirmSelection => self.confirm_selection(host),
            Action::FilterAppend(ch) => {
                self.filter_text.push(ch);
                self.reset_overlay_selection();
            }
            Action::FilterBackspace => {
                self.filter_text.pop();
                self.reset_overlay_selection();
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
            Action::SlashLogin(provider) => {
                match host.send(Command::AuthLoginOAuth {
                    command_id: command_id(),
                    provider,
                }) {
                    Ok(()) => self.status = "starting OAuth login".to_string(),
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            Action::SlashLogout(provider) => {
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
            _ => {}
        }
    }

    fn select_prev(&mut self) {
        match self.mode {
            AppMode::Tree => self.tree.select_prev(&self.filter_text),
            AppMode::Settings => self.settings.select_prev(&self.filter_text),
            AppMode::Sessions => self.sessions.select_prev(&self.filter_text),
            AppMode::Models => self.models.select_prev(&self.filter_text),
            _ => {}
        }
    }

    fn confirm_selection(&mut self, host: &mut HostdClient) {
        match self.mode {
            AppMode::Tree => self.navigate_selected_tree_entry(host),
            AppMode::Sessions => self.open_selected_session(host),
            AppMode::Models => self.apply_selected_model(host),
            AppMode::Settings => self.apply_selected_setting(host),
            AppMode::Status | AppMode::Help | AppMode::Chat | AppMode::Approval => {}
        }
    }

    fn reset_overlay_selection(&mut self) {
        self.sessions.list.selected = 0;
        self.models.list.selected = 0;
        self.settings.list.selected = 0;
        self.tree.list.selected = 0;
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

    fn navigate_selected_tree_entry(&mut self, host: &mut HostdClient) {
        let Some(session_id) = self.session_id.clone() else {
            self.status = "no active session".to_string();
            return;
        };
        let Some(entry_id) = self.tree.selected_entry_id(&self.filter_text) else {
            self.status = "no tree entry selected".to_string();
            return;
        };
        match host.send(Command::SessionNavigate {
            command_id: command_id(),
            session_id,
            entry_id,
        }) {
            Ok(()) => {
                self.clear_focus();
                self.status = "navigating session tree".to_string();
            }
            Err(err) => self.push_error(err.to_string()),
        }
    }

    fn apply_selected_model(&mut self, host: &mut HostdClient) {
        let Some(model) = self.models.selected_model(&self.filter_text) else {
            self.status = "no model selected".to_string();
            return;
        };
        let provider = model.provider.clone();
        let model_id = model.id.clone();
        match host.send(Command::ConfigSet {
            command_id: command_id(),
            default_provider: Some(provider.clone()),
            default_model: Some(model_id.clone()),
            default_thinking_level: None,
            active_tools: None,
            theme: None,
            hide_thinking_block: None,
            transport: None,
            compaction_enabled: None,
            compaction_reserve_tokens: None,
            compaction_keep_recent_tokens: None,
        }) {
            Ok(()) => {
                self.clear_focus();
                self.status = format!("switching model to {provider}/{model_id}");
            }
            Err(err) => self.push_error(err.to_string()),
        }
    }

    fn apply_selected_setting(&mut self, host: &mut HostdClient) {
        let Some(option) = self.settings.selected_option(&self.filter_text).cloned() else {
            self.status = "no setting selected".to_string();
            return;
        };
        let command = config_command_for_setting(option.action);
        match host.send(command) {
            Ok(()) => {
                self.clear_focus();
                self.status = format!("setting applied: {}", option.title);
                self.notify(NotificationLevel::Info, self.status.clone());
            }
            Err(err) => self.push_error(err.to_string()),
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
                self.tree.list.items.clear();
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
            CommandCatalogAction::Tree => {
                self.push_focus(AppMode::Tree);
                self.status = format!("{} session entries", self.tree.list.items.len());
            }
            CommandCatalogAction::Settings => {
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
                let provider = "anthropic";
                match host.send(Command::AuthLoginOAuth {
                    command_id: command_id(),
                    provider: provider.to_string(),
                }) {
                    Ok(()) => {
                        self.clear_focus();
                        self.status = format!("starting {provider} OAuth login");
                    }
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            CommandCatalogAction::Logout => {
                let provider = "anthropic";
                match host.send(Command::AuthLogout {
                    command_id: command_id(),
                    provider: provider.to_string(),
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
                match host.send(empty_config_set_with(|c| {
                    if let Command::ConfigSet {
                        default_thinking_level,
                        ..
                    } = c
                    {
                        *default_thinking_level = Some(level.clone());
                    }
                })) {
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
