use super::*;

impl AppState {
    pub(super) fn open_thought(&mut self, hit_id: u64) {
        if !self.timeline().thinking_visible {
            return;
        }
        let Some(key) = self.timeline().thought_key_for_hit(hit_id) else {
            return;
        };
        let Some(thought) = self.timeline().thought(&key) else {
            return;
        };
        self.thought_inspector
            .get_or_insert_with(crate::features::thought_inspector::ThoughtInspectorState::new)
            .open(&thought, self.last_tick);
        self.push_surface(SurfaceId::ThoughtInspector);
    }

    pub(super) fn dispatch_editor_action(&mut self, action: EditorAction) -> Vec<Effect> {
        let mut effects = Vec::new();
        match action {
            EditorAction::Submit => effects.extend(self.submit()),
            EditorAction::Cancel => effects.extend(self.cancel()),
            EditorAction::CancelSuggestions => self.editor.auto_complete.clear(),
            EditorAction::InsertChar(ch) => {
                self.editor.insert_char(ch);
                self.refresh_suggestions();
                let text = self.editor.text();
                if let Some(path) = local_image_path_from_paste(&text) {
                    effects.push(Effect::read_image_file_replacing(path, text));
                }
            }
            EditorAction::InsertPaste(text) => {
                if let Some(tb) = self.active_text_box() {
                    tb.insert_str(&text);
                } else if let Some(path) = local_image_path_from_paste(&text) {
                    self.editor.auto_complete.clear();
                    effects.push(Effect::read_image_file(path));
                } else {
                    self.editor.insert_paste(&text, &self.tui_config.editor);
                    self.refresh_suggestions();
                }
            }
            EditorAction::PasteImage => effects.push(Effect::read_clipboard_image()),
            EditorAction::InsertImage {
                filename,
                data,
                mime_type,
            } => {
                self.editor.insert_image(&filename, data, mime_type);
                self.refresh_suggestions();
                self.status = "image attached".to_string();
            }
            EditorAction::ReplaceDraftWithImage {
                expected_text,
                filename,
                data,
                mime_type,
            } => {
                if self.editor.text() == expected_text {
                    self.editor.restore_text("");
                    self.editor.insert_image(&filename, data, mime_type);
                    self.refresh_suggestions();
                    self.status = "image attached".to_string();
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
            EditorAction::DeleteWordBackward => {
                self.editor.delete_word_backward();
                self.refresh_suggestions();
            }
            EditorAction::DeleteWordForward => {
                self.editor.delete_word_forward();
                self.refresh_suggestions();
            }
            EditorAction::DeleteToLineStart => {
                self.editor.delete_to_line_start();
                self.refresh_suggestions();
            }
            EditorAction::DeleteToLineEnd => {
                self.editor.delete_to_line_end();
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
            EditorAction::CursorWordLeft => {
                self.editor.move_word_left();
                self.refresh_suggestions();
            }
            EditorAction::CursorWordRight => {
                self.editor.move_word_right();
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
            EditorAction::FollowUp => effects.extend(self.submit_follow_up()),
            EditorAction::Steer => effects.extend(self.submit_steer()),
            EditorAction::DequeueFollowUp => effects.extend(self.dequeue_follow_up()),
        }
        effects
    }

    pub(super) fn dispatch_timeline_action(&mut self, action: TimelineAction) -> Vec<Effect> {
        match action {
            TimelineAction::ScrollUp(n) => self.timeline_mut().scroll_up(n),
            TimelineAction::ScrollDown(n) => self.timeline_mut().scroll_down(n),
            TimelineAction::JumpLatest => self.timeline_mut().jump_latest(),
            TimelineAction::ToggleTool(index) => self.timeline_mut().toggle_tool(index),
            TimelineAction::OpenThought(hit_id) => self.open_thought(hit_id),
            TimelineAction::SelectionStart(point) => self.timeline_mut().start_selection(point),
            TimelineAction::SelectionUpdate(point) => self.timeline_mut().update_selection(point),
            TimelineAction::SelectionFinish { point, activation } => {
                let dragged = self.timeline_mut().finish_selection(point);
                if !dragged {
                    match activation {
                        Some(crate::app::command::TimelineActivation::Tool(hit_id)) => {
                            self.timeline_mut().toggle_tool(hit_id);
                        }
                        Some(crate::app::command::TimelineActivation::Thought(hit_id)) => {
                            self.open_thought(hit_id);
                        }
                        None => {}
                    }
                }
            }
            TimelineAction::CopySelection => {
                if let Some(text) = self.timeline().selected_text() {
                    return vec![Effect::copy_text(text, "timeline copied")];
                }
            }
        }
        Vec::new()
    }

    pub(super) fn dispatch_thought_inspector_action(
        &mut self,
        action: ThoughtInspectorAction,
    ) -> Vec<Effect> {
        match action {
            ThoughtInspectorAction::ScrollUp(amount) => {
                if let Some(inspector) = self.thought_inspector.as_mut() {
                    inspector.scroll_up(amount);
                }
            }
            ThoughtInspectorAction::ScrollDown(amount) => {
                if let Some(inspector) = self.thought_inspector.as_mut() {
                    inspector.scroll_down(amount);
                }
            }
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
            SurfaceAction::OpenUsage => {
                self.usage_scroll = 0;
                self.push_surface(SurfaceId::Usage);
                if let Some(session_id) = self.session_id().map(str::to_string) {
                    self.status = "refreshing usage".to_string();
                    let command_id = super::command_id();
                    self.session.pending.track(
                        command_id.clone(),
                        crate::app::pending::PendingCommandKind::UsageRefresh,
                    );
                    return vec![Effect::send(Command::StateSnapshot {
                        command_id,
                        session_id,
                    })];
                }
                self.status = "no active session for /usage".to_string();
            }
            SurfaceAction::OpenTodos => {
                self.todo_lists.reset_scroll();
                self.push_surface(SurfaceId::Todos);
                self.status = "todos".to_string();
            }
            SurfaceAction::TodoScrollUp(amount) => self.todo_lists.scroll_up(amount),
            SurfaceAction::TodoScrollDown(amount) => self.todo_lists.scroll_down(amount),
            SurfaceAction::OpenNotifications => {
                self.notifications.open_modal();
                self.push_surface(SurfaceId::Notifications);
                self.status = "notifications".to_string();
            }
            SurfaceAction::OpenTree => {
                self.tree.filter_mode = self.tui_config.tree.filter_mode.into();
                self.push_surface(SurfaceId::Tree);
                self.tree.rebuild_visible_for_filter();
                self.status = format!("{} session entries", self.tree.visible.rows.len());
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
                    let hint = crate::features::guidance_row::binding_hint(
                        self,
                        crate::input::command::CommandId::UiConfirm,
                    )
                    .unwrap_or_else(|| "unbound".to_string());
                    format!("{n} agent(s) — {hint} to view")
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
                if self.mode() == AppMode::Surface(SurfaceId::Sessions) {
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
                if self.mode() == AppMode::Surface(SurfaceId::Sessions) {
                    self.sessions.named_only = !self.sessions.named_only;
                    self.reset_overlay_selection();
                }
            }
            SessionAction::TogglePath => {
                if self.mode() == AppMode::Surface(SurfaceId::Sessions) {
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
            ApprovalAction::SelectNext => {
                self.approvals.select_next();
                Vec::new()
            }
            ApprovalAction::SelectPrev => {
                self.approvals.select_prev();
                Vec::new()
            }
            ApprovalAction::ConfirmSelected => {
                if let Some(decision) = self.approvals.selected_decision() {
                    self.respond_approval(decision)
                } else {
                    Vec::new()
                }
            }
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
            NotificationAction::DismissVisible => self.notifications.dismiss_visible(
                self.last_tick,
                self.session.id.as_deref(),
                self.agent_panel.active_agent_instance_id.as_deref(),
            ),
            NotificationAction::ToggleScope => self.notifications.toggle_view_scope(),
            NotificationAction::SelectPrev => self.notifications.select_prev(),
            NotificationAction::SelectNext => self.notifications.select_next(),
            NotificationAction::CopySelected => {
                if let Some((id, message)) = self
                    .notifications
                    .selected_copy_payload(self.session.id.as_deref())
                {
                    return vec![Effect::copy_to_clipboard(id, message)];
                }
            }
            NotificationAction::Copy(id) => {
                if let Some(message) = self.notifications.message(id) {
                    return vec![Effect::copy_to_clipboard(id, message)];
                }
            }
            NotificationAction::ScrollUp(amount) => self.notifications.scroll_up(amount),
            NotificationAction::ScrollDown(amount) => self.notifications.scroll_down(amount),
        }
        Vec::new()
    }
}

fn local_image_path_from_paste(text: &str) -> Option<std::path::PathBuf> {
    let value = text.trim();
    if value.is_empty() || value.contains(['\n', '\r']) {
        return None;
    }
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value);
    let path = std::path::PathBuf::from(value);
    if !path.is_absolute() {
        return None;
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp").then_some(path)
}
