use piko_protocol::{ApprovalDecision, Command};

use crate::{
    action::Action,
    app::{AppMode, AppState, command_id, config_command_for_setting, empty_config_set_with},
    host::HostdClient,
    input::completion,
    notification::NotificationLevel,
    surfaces::commands::CommandAction,
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
                self.completions.clear();
                self.selected_completion = 0;
            }
            Action::InsertChar(ch) => {
                self.editor.insert_char(ch);
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
                self.accept_suggestion();
                self.submit(host);
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
                self.push_focus(AppMode::Commands);
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
        !self.completions.is_empty() && self.mode == AppMode::Chat
    }

    fn select_next(&mut self) {
        match self.mode {
            AppMode::Commands => self.commands.select_next(&self.filter_text),
            AppMode::Tree => self.tree.select_next(&self.filter_text),
            AppMode::Settings => self.settings.select_next(&self.filter_text),
            AppMode::Sessions => self.sessions.select_next(&self.filter_text),
            AppMode::Models => self.models.select_next(&self.filter_text),
            _ => {}
        }
    }

    fn select_prev(&mut self) {
        match self.mode {
            AppMode::Commands => self.commands.select_prev(&self.filter_text),
            AppMode::Tree => self.tree.select_prev(&self.filter_text),
            AppMode::Settings => self.settings.select_prev(&self.filter_text),
            AppMode::Sessions => self.sessions.select_prev(&self.filter_text),
            AppMode::Models => self.models.select_prev(&self.filter_text),
            _ => {}
        }
    }

    fn confirm_selection(&mut self, host: &mut HostdClient) {
        match self.mode {
            AppMode::Commands => {
                let action = self.commands.selected_action(&self.filter_text);
                if let Some(action) = action {
                    self.run_command_action(host, action);
                }
            }
            AppMode::Tree => self.navigate_selected_tree_entry(host),
            AppMode::Sessions => self.open_selected_session(host),
            AppMode::Models => self.apply_selected_model(host),
            AppMode::Settings => self.apply_selected_setting(host),
            AppMode::Status | AppMode::Help | AppMode::Chat | AppMode::Approval => {}
        }
    }

    fn reset_overlay_selection(&mut self) {
        self.commands.list.selected = 0;
        self.sessions.list.selected = 0;
        self.models.list.selected = 0;
        self.settings.list.selected = 0;
        self.tree.list.selected = 0;
    }

    // ── input helpers ─────────────────────────────────────────────────────────

    fn submit(&mut self, host: &mut HostdClient) {
        let Some(text) = self.editor.take_trimmed() else {
            return;
        };
        self.refresh_suggestions();
        if text.starts_with('/') {
            if self.try_slash_command(host, &text) {
                return;
            } else {
                self.editor.replace_range(0, 0, &text);
                self.status = format!("Unknown slash command: {}", text);
                self.notify(NotificationLevel::Error, format!("Unknown slash command: {}", text));
                return;
            }
        }
        let Some(session_id) = self.session_id.clone() else {
            self.status = "no active session yet".to_string();
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
            self.status = "no active turn to cancel".to_string();
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
        self.completions =
            completion::complete(&self.cwd, self.editor.text(), self.editor.cursor());
        self.selected_completion = self
            .selected_completion
            .min(self.completions.len().saturating_sub(1));
    }

    fn select_suggestion_next(&mut self) {
        if !self.completions.is_empty() {
            self.selected_completion =
                (self.selected_completion + 1).min(self.completions.len() - 1);
        }
    }

    fn select_suggestion_prev(&mut self) {
        self.selected_completion = self.selected_completion.saturating_sub(1);
    }

    fn accept_suggestion(&mut self) {
        let Some(completion) = self.completions.get(self.selected_completion).cloned() else {
            return;
        };
        self.editor
            .replace_range(completion.start, completion.end, &completion.replacement);
        self.refresh_suggestions();
    }

    fn history_prev(&mut self) {
        if self.editor.is_empty() {
            self.editor.history_prev();
            self.refresh_suggestions();
        } else {
            self.timeline.scroll_up(1);
        }
    }

    fn history_next(&mut self) {
        self.editor.history_next();
        self.refresh_suggestions();
    }

    // ── session operations ────────────────────────────────────────────────────

    fn request_sessions(&mut self, host: &mut HostdClient) {
        match host.send(Command::SessionList {
            command_id: command_id(),
        }) {
            Ok(()) => {
                self.push_focus(AppMode::Sessions);
                self.status = "loading sessions".to_string();
            }
            Err(err) => self.push_error(err.to_string()),
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
        let Some(session_id) = self.sessions.selected_session_id(&self.filter_text) else {
            self.status = "no session selected".to_string();
            return;
        };
        match host.send(Command::SessionOpen {
            command_id: command_id(),
            session_id,
        }) {
            Ok(()) => {
                self.clear_focus();
                self.status = "opening session".to_string();
            }
            Err(err) => self.push_error(err.to_string()),
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

    fn run_command_action(&mut self, host: &mut HostdClient, action: CommandAction) {
        match action {
            CommandAction::Help => {
                self.push_focus(AppMode::Help);
                self.status = "help".to_string();
            }
            CommandAction::Sessions => self.request_sessions(host),
            CommandAction::Models => self.request_models(host),
            CommandAction::SessionTree => {
                self.push_focus(AppMode::Tree);
                self.status = format!("{} session entries", self.tree.list.items.len());
            }
            CommandAction::Settings => {
                self.push_focus(AppMode::Settings);
                self.status = "settings".to_string();
            }
            CommandAction::Status => {
                self.push_focus(AppMode::Status);
                self.status = "status".to_string();
            }
            CommandAction::NewSession => {
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
            CommandAction::ForkSession => {
                let entry_id = self.tree.selected_entry_id(&self.filter_text);
                self.fork_session(host, entry_id);
            }
            CommandAction::CloneSession => self.fork_session(host, None),
            CommandAction::Login(provider) => {
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
            CommandAction::Logout(provider) => {
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
            CommandAction::Compact => {
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
            CommandAction::Thinking(level) => {
                match host.send(empty_config_set_with(|c| {
                    if let Command::ConfigSet {
                        default_thinking_level,
                        ..
                    } = c
                    {
                        *default_thinking_level = Some(level.to_string());
                    }
                })) {
                    Ok(()) => {
                        self.clear_focus();
                        self.status = format!("thinking level {level}");
                    }
                    Err(err) => self.push_error(err.to_string()),
                }
            }
            CommandAction::ToggleToolsExpanded => {
                self.timeline.tools_expanded = !self.timeline.tools_expanded;
                self.clear_focus();
                self.status = if self.timeline.tools_expanded {
                    "tool details expanded".to_string()
                } else {
                    "tool details folded".to_string()
                };
                self.notify(NotificationLevel::Info, self.status.clone());
            }
            CommandAction::ClearNotifications => {
                self.notifications.clear();
                self.clear_focus();
                self.status = "notifications cleared".to_string();
            }
            CommandAction::Quit => self.quit = true,
        }
    }
}
