use super::*;

impl AppState {
    pub(super) fn dispatch_editor_action(&mut self, action: EditorAction) -> Vec<Effect> {
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
                let submit_on_accept = self
                    .editor
                    .auto_complete
                    .list
                    .selected_item()
                    .is_some_and(|item| item.submit_on_accept);
                self.accept_suggestion();
                if submit_on_accept {
                    effects.extend(self.submit());
                }
            }
            EditorAction::SuggestionSelectNext => self.select_suggestion_next(),
            EditorAction::SuggestionSelectPrev => self.select_suggestion_prev(),
        }
        effects
    }

    pub(super) fn dispatch_timeline_action(&mut self, action: TimelineAction) -> Vec<Effect> {
        match action {
            TimelineAction::ScrollUp(n) => self.timeline.scroll_up(n),
            TimelineAction::ScrollDown(n) => self.timeline.scroll_down(n),
            TimelineAction::JumpLatest => self.timeline.jump_latest(),
            TimelineAction::ToggleTool(index) => self.timeline.toggle_tool(index),
        }
        Vec::new()
    }

    pub(super) fn dispatch_surface_action(&mut self, action: SurfaceAction) -> Vec<Effect> {
        match action {
            SurfaceAction::OpenSettings => {
                let snap = self.settings_snapshot();
                self.settings.open_root(&snap);
                self.push_surface(SurfaceId::Settings);
                self.status = "settings".to_string();
                let mut effects = Vec::new();
                for namespace in ["host", "tui"] {
                    let command_id = super::command_id();
                    self.session.pending.track(
                        command_id.clone(),
                        crate::app::pending::PendingCommandKind::BootstrapConfig,
                    );
                    effects.push(Effect::send(Command::ConfigGet {
                        command_id,
                        namespace: namespace.to_string(),
                    }));
                }
                return effects;
            }
            SurfaceAction::OpenStatus => {
                self.push_surface(SurfaceId::Status);
                self.status = "status".to_string();
            }
            SurfaceAction::OpenTree => {
                self.tree.filter_mode = self.tui_config.tree.filter_mode.into();
                self.push_surface(SurfaceId::Tree);
                self.tree.rebuild_visible_for_filter();
                self.status = format!("{} session entries", self.tree.visible.rows.len());
            }
            SurfaceAction::OpenThinking => {
                let active = self
                    .model
                    .active_thinking_level
                    .clone()
                    .or_else(|| self.host_settings.thinking_level.clone());
                self.thinking.prepare(active.as_deref());
                self.push_surface(SurfaceId::Thinking);
                self.status = "thinking level".to_string();
            }
            SurfaceAction::OpenAgents => {
                self.agent_panel.prepare_for_switch();
                self.push_surface(SurfaceId::Agents);
                let n = self.agent_panel.list.len();
                self.status = if self.agent_panel.is_loading() {
                    "loading agents".to_string()
                } else if n == 0 {
                    "no agents in session".to_string()
                } else {
                    format!("{n} agent(s) — Enter to view")
                };
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

    pub(super) fn dispatch_session_action(&mut self, action: SessionAction) -> Vec<Effect> {
        let mut effects = Vec::new();
        match action {
            SessionAction::RequestList => effects.extend(self.request_sessions()),
            SessionAction::ToggleScope => {
                if self.mode == AppMode::Surface(SurfaceId::Sessions) {
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
                if self.mode == AppMode::Surface(SurfaceId::Sessions) {
                    self.sessions.named_only = !self.sessions.named_only;
                    self.reset_overlay_selection();
                }
            }
            SessionAction::TogglePath => {
                if self.mode == AppMode::Surface(SurfaceId::Sessions) {
                    self.sessions.show_path = !self.sessions.show_path;
                }
            }
        }
        effects
    }

    pub(super) fn dispatch_model_action(&mut self, action: ModelAction) -> Vec<Effect> {
        match action {
            ModelAction::RequestList => self.request_models(),
        }
    }

    pub(super) fn dispatch_tree_action(&mut self, action: TreeAction) -> Vec<Effect> {
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

    pub(super) fn dispatch_approval_action(&mut self, action: ApprovalAction) -> Vec<Effect> {
        match action {
            ApprovalAction::Respond(decision) => self.respond_approval(decision),
        }
    }

    pub(super) fn dispatch_tool_interaction_action(
        &mut self,
        action: ToolInteractionAction,
    ) -> Vec<Effect> {
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
            ToolInteractionAction::GotoStep(step) => {
                if let Some(interaction) = self.interactions.front_mut() {
                    if interaction.workflow.input_active() {
                        interaction.workflow.set_input_active(false);
                    }
                    interaction.workflow.goto_step(step);
                }
                Vec::new()
            }
            ToolInteractionAction::SelectNext => {
                if let Some(interaction) = self.interactions.front_mut() {
                    interaction.workflow.select_next();
                }
                Vec::new()
            }
            ToolInteractionAction::SelectPrev => {
                if let Some(interaction) = self.interactions.front_mut() {
                    interaction.workflow.select_prev();
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

    pub(super) fn dispatch_notification_action(
        &mut self,
        action: NotificationAction,
    ) -> Vec<Effect> {
        match action {
            NotificationAction::Clear => self.notifications.clear(),
        }
        Vec::new()
    }
}
