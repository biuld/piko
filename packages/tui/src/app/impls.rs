use super::*;

impl AppState {
    pub fn new(
        cwd: PathBuf,
        requested_session_id: Option<String>,
        continue_session: bool,
        initial_options: InitialOptions,
    ) -> Self {
        let awaiting_session = requested_session_id.is_some() || continue_session;
        let session = SessionUiState {
            initializing: awaiting_session,
            requested_id: requested_session_id,
            continue_requested: continue_session,
            ..Default::default()
        };
        let model = ModelUiState {
            active_model_id: initial_options.model_id.clone(),
            active_provider: initial_options.provider.clone(),
            active_thinking_level: initial_options.thinking_level.clone(),
            host_context_window: None,
            providers: Vec::new(),
        };
        // Booting is loading even on a cold start. Required bootstrap results
        // transition the panel to the authoritative no-session empty state.
        let agent_panel = AgentPanelState::default();
        Self {
            cwd,
            initial_options,
            session,
            model,
            mode: AppMode::Chat,
            focus_manager: FocusManager::new(),
            quit: false,
            last_tick: Instant::now(),
            editor: Editor::default(),
            command_catalog: Vec::new(),
            status: "starting hostd".to_string(),
            queue_status: QueueStatus::default(),
            spinner_frame: 0,
            timeline: Timeline::new(),
            agent_timelines: HashMap::new(),
            approvals: ApprovalPanel::new(),
            mcp: crate::features::mcp::McpPanel::new(),
            diagnostics: crate::features::diagnostics::DiagnosticsPanel::new(),
            last_turn_id: None,
            last_turn_diff: None,
            interactions: ToolInteractionPanel::new(),
            sessions: SessionList::new(),
            agents: crate::features::agent_list::AgentList::new(),
            models: ModelSelector::new(),
            settings: SettingsPanel::new(),
            tree: TreePanel::new(),
            summary_prompt: None,
            auth_selector: AuthSelector::new(&[]),
            agent_panel,
            notifications: NotificationCenter::default(),
            tui_config: TuiConfig::default(),
            theme: Theme::dark(),
        }
    }

    // ── accessors ─────────────────────────────────────────────────────────────

    pub fn active_text_box(&mut self) -> Option<&mut TextBox> {
        match self.focus_manager.active_mode() {
            AppMode::AuthSelector => match &mut self.auth_selector.state {
                crate::features::auth_selector::AuthSelectorState::ApiKeyInput {
                    input, ..
                } => Some(input),
                _ => None,
            },
            AppMode::SummaryPrompt => {
                if let Some(workflow) = &mut self.summary_prompt
                    && !workflow.questions.is_empty()
                {
                    let q = &mut workflow.questions[workflow.active_question_idx];
                    if q.is_input_active {
                        return Some(&mut q.input_value);
                    }
                }
                None
            }
            AppMode::Tree => {
                if let Some(editor) = &mut self.tree.label_editor {
                    Some(&mut editor.input)
                } else {
                    None
                }
            }
            AppMode::ToolInteraction => {
                if let Some(interaction) = self.interactions.front_mut()
                    && !interaction.workflow.questions.is_empty()
                {
                    let q = &mut interaction.workflow.questions
                        [interaction.workflow.active_question_idx];
                    if q.is_input_active {
                        return Some(&mut q.input_value);
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session.id.as_deref()
    }

    pub fn active_turn_id(&self) -> Option<&str> {
        let agent_instance_id = self.agent_panel.active_agent_instance_id.as_deref()?;
        self.session
            .active_turns
            .get(agent_instance_id)
            .map(|t| t.turn_id.as_str())
    }

    /// Per-agent foreground work projection (F-22).
    ///
    /// Uses the sole protocol path [`piko_protocol::AgentForeground::project`]
    /// so Queued / Running / RequiresAction / Cancelling match client-core.
    pub fn agent_foreground(
        &self,
        agent_instance_id: &str,
        activity: &piko_protocol::AgentActivity,
    ) -> piko_protocol::AgentForeground {
        let blocked = self
            .approvals
            .pending
            .iter()
            .any(|a| a.agent_instance_id == agent_instance_id)
            || self.interactions.pending_for_agent(agent_instance_id);
        let turn_status = self
            .session
            .active_turns
            .get(agent_instance_id)
            .map(|t| t.status);
        piko_protocol::AgentForeground::project(blocked, turn_status, Some(activity))
    }

    pub fn cwd(&self) -> PathBuf {
        self.cwd.clone()
    }

    pub fn push_focus(&mut self, mode: AppMode) {
        self.focus_manager.push(mode);
        self.mode = self.focus_manager.active_mode();
        if mode != AppMode::SummaryPrompt {
            self.clear_filter_for_mode(mode);
        }
        // Sync widget panel focus flags
        self.agent_panel.focus = mode == AppMode::AgentPanel;
    }

    pub fn pop_focus(&mut self) {
        let popped = self.focus_manager.pop();
        self.mode = self.focus_manager.active_mode();
        if popped != Some(AppMode::SummaryPrompt)
            && let Some(mode) = popped
        {
            self.clear_filter_for_mode(mode);
        }
    }

    pub fn clear_focus(&mut self) {
        self.focus_manager.clear_to_chat();
        self.mode = self.focus_manager.active_mode();
        self.clear_all_filters();
        self.agent_panel.focus = false;
    }

    pub(crate) fn clear_filter_for_mode(&mut self, mode: AppMode) {
        match mode {
            AppMode::Sessions => self.sessions.filter.clear(),
            AppMode::AgentList => self.agents.filter.clear(),
            AppMode::Tree => self.tree.filter.clear(),
            AppMode::Models => self.models.filter.clear(),
            AppMode::Settings => self.settings.filter.clear(),
            AppMode::AuthSelector => self.auth_selector.filter.clear(),
            _ => {}
        }
    }

    pub(crate) fn clear_all_filters(&mut self) {
        self.sessions.filter.clear();
        self.tree.filter.clear();
        self.models.filter.clear();
        self.settings.filter.clear();
        self.auth_selector.filter.clear();
    }

    pub(crate) fn active_filter_mut(&mut self) -> Option<&mut String> {
        match self.mode {
            AppMode::Sessions => Some(&mut self.sessions.filter),
            AppMode::AgentList => Some(&mut self.agents.filter),
            AppMode::Tree => Some(&mut self.tree.filter),
            AppMode::Models => Some(&mut self.models.filter),
            AppMode::Settings => Some(&mut self.settings.filter),
            AppMode::AuthSelector => match self.auth_selector.state {
                crate::features::auth_selector::AuthSelectorState::Menu => {
                    Some(&mut self.auth_selector.filter)
                }
                crate::features::auth_selector::AuthSelectorState::ApiKeyInput { .. } => None,
            },
            _ => None,
        }
    }
}
