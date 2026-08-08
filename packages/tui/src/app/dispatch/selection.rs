use super::*;

impl AppState {
    pub fn has_suggestions(&self) -> bool {
        self.editor.auto_complete.is_active() && self.mode == AppMode::Chat
    }

    pub(super) fn select_next(&mut self) {
        match self.mode.as_surface() {
            Some(SurfaceId::Tree) => self.tree.select_next_filtered(),
            Some(SurfaceId::Settings) => self.settings.select_next(),
            Some(SurfaceId::Sessions) => self.sessions.select_next(),
            Some(SurfaceId::Agents) => self.agent_panel.select_next(),
            Some(SurfaceId::Models) => self.models.select_next(),
            Some(SurfaceId::AuthSelector) => self.auth_selector.select_next(),
            Some(SurfaceId::Diagnostics) => self.diagnostics.scroll_down(1),
            _ => {}
        }
    }

    pub(super) fn select_prev(&mut self) {
        match self.mode.as_surface() {
            Some(SurfaceId::Tree) => self.tree.select_prev_filtered(),
            Some(SurfaceId::Settings) => self.settings.select_prev(),
            Some(SurfaceId::Sessions) => self.sessions.select_prev(),
            Some(SurfaceId::Agents) => self.agent_panel.select_prev(),
            Some(SurfaceId::Models) => self.models.select_prev(),
            Some(SurfaceId::AuthSelector) => self.auth_selector.select_prev(),
            Some(SurfaceId::Diagnostics) => self.diagnostics.scroll_up(1),
            _ => {}
        }
    }

    pub(super) fn request_turn_diff(&mut self) -> Vec<Effect> {
        let Some(session_id) = self.session.id.clone() else {
            self.status = "no active session for /diff".to_string();
            return Vec::new();
        };
        let turn_id = self
            .active_turn_id()
            .map(str::to_string)
            .or_else(|| self.last_turn_id.clone());
        let Some(turn_id) = turn_id else {
            if let Some(diff) = self.last_turn_diff.clone() {
                self.diagnostics.set_diff(&diff);
                self.push_surface(SurfaceId::Diagnostics);
                self.status = "turn diff".to_string();
            } else {
                self.status = "no turn id for /diff".to_string();
            }
            return Vec::new();
        };
        // Prefer cached push if it matches the requested turn.
        if let Some(diff) = self.last_turn_diff.as_ref()
            && diff.turn_id == turn_id
        {
            self.diagnostics.set_diff(diff);
            self.push_surface(SurfaceId::Diagnostics);
            self.status = "turn diff".to_string();
            return Vec::new();
        }
        self.status = format!("fetching diff for turn {turn_id}");
        vec![Effect::send(Command::TurnDiffGet {
            command_id: command_id(),
            session_id,
            turn_id,
        })]
    }

    pub(super) fn request_prompt_debug(&mut self) -> Vec<Effect> {
        let Some(session_id) = self.session.id.clone() else {
            self.status = "no active session for /prompt-debug".to_string();
            return Vec::new();
        };
        let Some(agent_instance_id) = self.agent_panel.active_agent_instance_id.clone() else {
            self.status = "no active agent for /prompt-debug".to_string();
            return Vec::new();
        };
        self.status = "fetching prompt debug".to_string();
        vec![Effect::send(Command::PromptDebugGet {
            command_id: command_id(),
            session_id,
            agent_instance_id,
        })]
    }

    pub(super) fn request_rollout_page(&mut self) -> Vec<Effect> {
        let Some(session_id) = self.session.id.clone() else {
            self.status = "no active session for /rollout".to_string();
            return Vec::new();
        };
        let Some(agent_instance_id) = self.agent_panel.active_agent_instance_id.clone() else {
            self.status = "no active agent for /rollout".to_string();
            return Vec::new();
        };
        self.status = "fetching rollout page".to_string();
        vec![Effect::send(Command::RolloutPageGet {
            command_id: command_id(),
            session_id,
            agent_instance_id,
            after_cursor: None,
            limit: Some(50),
        })]
    }

    pub(super) fn close_surface(&mut self) {
        match self.mode {
            AppMode::Surface(SurfaceId::SummaryPrompt) => {
                self.summary_prompt = None;
                self.pop_focus();
            }
            AppMode::Surface(SurfaceId::Tree) if self.tree.label_editor.is_some() => {
                self.tree.cancel_label_edit();
            }
            AppMode::Surface(SurfaceId::ToolInteraction) => {
                if let Some(interaction) = self.interactions.front_mut()
                    && interaction.workflow.input_active()
                {
                    interaction.workflow.set_input_active(false);
                    return;
                }
                self.pop_focus();
            }
            AppMode::Surface(SurfaceId::Settings) => {
                if !self.settings.pop() {
                    self.pop_focus();
                }
            }
            AppMode::Chat => {}
            _ => self.pop_focus(),
        }
    }

    pub(super) fn select_surface_next(&mut self) {
        match self.mode {
            AppMode::Surface(SurfaceId::SummaryPrompt) => {
                if let Some(workflow) = self.summary_prompt.as_mut() {
                    workflow.next_step();
                }
            }
            _ => self.select_next(),
        }
    }

    pub(super) fn select_surface_prev(&mut self) {
        match self.mode {
            AppMode::Surface(SurfaceId::SummaryPrompt) => {
                if let Some(workflow) = self.summary_prompt.as_mut() {
                    workflow.prev_step();
                }
            }
            _ => self.select_prev(),
        }
    }

    pub(super) fn append_active_filter(&mut self, ch: char) {
        if let Some(text_box) = self.active_text_box() {
            text_box.insert_char(ch);
            return;
        }

        if let Some(filter) = self.active_filter_mut() {
            filter.push(ch);
        }

        match self.mode {
            AppMode::Surface(SurfaceId::Tree) => {
                self.tree.rebuild_visible_for_filter();
            }
            AppMode::Surface(SurfaceId::Sessions) => self.sessions.list.selected = 0,
            AppMode::Surface(SurfaceId::Agents) => self.agent_panel.reset_selection(),
            AppMode::Surface(SurfaceId::Models) => self.models.reset(),
            AppMode::Surface(SurfaceId::Settings) => self.settings.reset_selection(),
            AppMode::Surface(SurfaceId::AuthSelector) => {
                if let Some(frame) = self.auth_selector.menu.stack.last_mut() {
                    frame.list.selected = 0;
                }
            }
            _ => {}
        }
    }

    pub(super) fn backspace_active_filter(&mut self) {
        if let Some(text_box) = self.active_text_box() {
            text_box.backspace();
            return;
        }

        if let Some(filter) = self.active_filter_mut() {
            filter.pop();
        }

        match self.mode {
            AppMode::Surface(SurfaceId::Tree) => {
                self.tree.rebuild_visible_for_filter();
            }
            AppMode::Surface(SurfaceId::Sessions) => self.sessions.list.selected = 0,
            AppMode::Surface(SurfaceId::Agents) => self.agent_panel.reset_selection(),
            AppMode::Surface(SurfaceId::Models) => self.models.reset(),
            AppMode::Surface(SurfaceId::Settings) => self.settings.reset_selection(),
            AppMode::Surface(SurfaceId::AuthSelector) => {
                if let Some(frame) = self.auth_selector.menu.stack.last_mut() {
                    frame.list.selected = 0;
                }
            }
            _ => {}
        }
    }

    pub(super) fn cycle_tree_filter(&mut self, delta: u8) {
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

    pub(super) fn confirm_selection(&mut self) -> Vec<Effect> {
        if self
            .focus_manager
            .active_mode()
            .is_surface(SurfaceId::SummaryPrompt)
        {
            return self.confirm_summary_prompt();
        }

        if self.mode.is_surface(SurfaceId::Tree) && self.tree.label_editor.is_some() {
            return self.confirm_tree_label_edit();
        }

        match self.mode.as_surface() {
            Some(SurfaceId::Tree) => self.confirm_tree_entry(),
            Some(SurfaceId::Sessions) => self.open_selected_session(),
            Some(SurfaceId::Agents) => self.apply_selected_agent_view(),
            Some(SurfaceId::Models) => self.apply_selected_model(),
            Some(SurfaceId::Settings) => self.apply_selected_setting(),
            Some(SurfaceId::AuthSelector) => self.confirm_auth_selection(),
            Some(
                SurfaceId::Status
                | SurfaceId::Mcp
                | SurfaceId::Diagnostics
                | SurfaceId::Help
                | SurfaceId::Approval
                | SurfaceId::ToolInteraction
                | SurfaceId::SummaryPrompt,
            )
            | None => Vec::new(),
        }
    }

    pub(super) fn apply_selected_agent_view(&mut self) -> Vec<Effect> {
        if self.agent_panel.is_loading() {
            self.status = "agents still loading".to_string();
            return Vec::new();
        }
        let Some(agent) = self.agent_panel.selected_agent().cloned() else {
            self.status = "no agent selected".to_string();
            return Vec::new();
        };
        self.dispatch_agent_panel_action(crate::app::command::AgentPanelAction::Subscribe {
            agent_instance_id: agent.agent_instance_id,
            agent_id: agent.agent_id,
        })
    }

    pub(super) fn reset_overlay_selection(&mut self) {
        self.sessions.list.selected = 0;
        self.models.reset();
        let snap = self.settings_snapshot();
        self.settings.open_root(&snap);
        self.tree.selected_idx = 0;
    }

    pub(super) fn dispatch_agent_panel_action(
        &mut self,
        action: crate::app::command::AgentPanelAction,
    ) -> Vec<Effect> {
        match action {
            crate::app::command::AgentPanelAction::Subscribe {
                agent_instance_id,
                agent_id,
            } => {
                let session_id = match self.session.id.clone() {
                    Some(id) => id,
                    None => return vec![],
                };
                self.clear_focus();
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
