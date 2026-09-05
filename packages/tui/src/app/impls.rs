use super::*;
use crate::navigation::FocusManagerExt;

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
            focus_manager: FocusManager::new(AppMode::Chat),
            quit: false,
            last_tick: Instant::now(),
            hovered: None,
            pointer_position: None,
            pointer_left_down: false,
            editor: Editor::default(),
            binding_registry: BindingRegistry::default(),
            command_catalog: crate::app::command::merge_command_catalog(&[]),
            status: "starting hostd".to_string(),
            queue_status: QueueStatus::default(),
            agent_usage: Vec::new(),
            usage_scroll: 0,
            spinner_frame: 0,
            timelines: TimelineStore::new(),
            approvals: ApprovalPanel::new(),
            mcp: crate::features::mcp::McpPanel::new(),
            processes: crate::features::processes::ProcessPanel::new(),
            diagnostics: crate::features::diagnostics::DiagnosticsPanel::new(),
            history: crate::features::history::HistoryPanel::default(),
            last_root_input_id: None,
            last_agent_work_diff: None,
            interactions: ToolInteractionPanel::new(),
            sessions: SessionList::new(),
            models: ModelSelector::new(),
            thinking: ThinkingSelector::new(),
            thought_inspector: None,
            pending_model: None,
            settings: SettingsPanel::new(),
            tree: TreePanel::new(),
            summary_prompt: None,
            auth_selector: AuthSelector::new(&[], &[]),
            tree_fork_mode: false,
            agent_panel,
            notifications: NotificationCenter::default(),
            todo_lists: crate::features::todos::TodoListsState::default(),
            tui_config: TuiConfig::default(),
            host_settings: HostRuntimeSettings::default(),
            theme: Theme::load(&TuiConfig::default().theme.name),
        }
    }

    /// Attach the process-local terminal profile before event processing.
    pub fn configure_terminal_profile(&mut self, profile: crate::terminal::TerminalProfile) {
        let color = profile.color;
        self.binding_registry =
            BindingRegistry::compile_with_diagnostics(profile, Some(&self.tui_config.keybindings));
        self.theme = self.theme.for_color_level(color);
    }

    /// Whether managed/user feature `todo` is enabled (default true).
    pub fn todo_feature_enabled(&self) -> bool {
        if let Some(v) = self.host_settings.managed_features.get("todo") {
            return *v;
        }
        self.host_settings
            .features
            .get("todo")
            .copied()
            .unwrap_or(true)
    }

    // ── accessors ─────────────────────────────────────────────────────────────

    /// Current input owner, derived from the sole authoritative focus stack.
    pub fn mode(&self) -> AppMode {
        self.focus_manager.active_mode()
    }

    pub fn timeline(&self) -> &Timeline {
        self.timelines.active()
    }

    pub fn timeline_mut(&mut self) -> &mut Timeline {
        self.timelines.active_mut()
    }

    pub(crate) fn reconcile_thought_inspector(&mut self) {
        let Some(key) = self
            .thought_inspector
            .as_ref()
            .map(|inspector| inspector.key().clone())
        else {
            if self.mode().is_surface(SurfaceId::ThoughtInspector) {
                self.pop_focus();
            }
            return;
        };
        let present = self.timeline().thinking_visible && self.timeline().thought(&key).is_some();
        if !present && self.mode().is_surface(SurfaceId::ThoughtInspector) {
            self.thought_inspector = None;
            self.pop_focus();
        }
    }

    pub(crate) fn advance_thought_inspector(&mut self, now: Instant) {
        if !self.mode().is_surface(SurfaceId::ThoughtInspector) {
            return;
        }
        let Some(key) = self
            .thought_inspector
            .as_ref()
            .map(|inspector| inspector.key().clone())
        else {
            self.reconcile_thought_inspector();
            return;
        };
        let thought = self.timeline().thought(&key);
        if let Some(thought) = thought {
            if let Some(inspector) = self.thought_inspector.as_mut() {
                inspector.advance_reveal_to(&thought, now);
            }
        } else {
            self.reconcile_thought_inspector();
        }
    }

    pub fn active_text_box(&mut self) -> Option<&mut TextBox> {
        match self.focus_manager.active_mode() {
            AppMode::Surface(SurfaceId::AuthSelector) => match &mut self.auth_selector.state {
                crate::features::auth_selector::AuthSelectorState::ApiKeyInput {
                    input, ..
                } => Some(input),
                _ => None,
            },
            AppMode::Surface(SurfaceId::SummaryPrompt) => {
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
            AppMode::Surface(SurfaceId::Tree) => {
                if let Some(editor) = &mut self.tree.label_editor {
                    Some(&mut editor.input)
                } else {
                    None
                }
            }
            AppMode::Surface(SurfaceId::ToolInteraction) => {
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

    /// Whether the current focus owner exposes an editable text sink.
    ///
    /// Input routing uses this shared predicate while building the immutable
    /// scope stack; it must not take a mutable borrow merely to answer a
    /// presence question.
    pub fn active_text_box_is_present(&self) -> bool {
        match self.focus_manager.active_mode() {
            AppMode::Surface(SurfaceId::AuthSelector) => matches!(
                self.auth_selector.state,
                crate::features::auth_selector::AuthSelectorState::ApiKeyInput { .. }
            ),
            AppMode::Surface(SurfaceId::SummaryPrompt) => self
                .summary_prompt
                .as_ref()
                .is_some_and(|workflow| workflow.input_active()),
            AppMode::Surface(SurfaceId::Tree) => self.tree.label_editor.is_some(),
            AppMode::Surface(SurfaceId::ToolInteraction) => self
                .interactions
                .front()
                .is_some_and(|interaction| interaction.workflow.input_active()),
            _ => false,
        }
    }

    pub fn accepts_text_paste(&self) -> bool {
        match self.focus_manager.active_mode() {
            AppMode::Chat => true,
            AppMode::Surface(SurfaceId::AuthSelector) => matches!(
                self.auth_selector.state,
                crate::features::auth_selector::AuthSelectorState::ApiKeyInput { .. }
            ),
            AppMode::Surface(SurfaceId::SummaryPrompt) => self
                .summary_prompt
                .as_ref()
                .is_some_and(|workflow| workflow.input_active()),
            AppMode::Surface(SurfaceId::Tree) => self.tree.label_editor.is_some(),
            AppMode::Surface(SurfaceId::ToolInteraction) => self
                .interactions
                .front()
                .is_some_and(|interaction| interaction.workflow.input_active()),
            _ => false,
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session.id.as_deref()
    }

    pub fn active_root_input_id(&self) -> Option<&str> {
        let agent_instance_id = self.agent_panel.active_agent_instance_id.as_deref()?;
        self.session
            .agent_work
            .get(agent_instance_id)?
            .active_work
            .as_ref()
            .map(|work| work.root_input_id.as_str())
    }

    /// Per-agent foreground work projection (F-22 / F-51).
    ///
    /// The host-authored `AgentWorkSnapshot` is the sole foreground authority.
    pub fn agent_foreground(&self, agent_instance_id: &str) -> piko_protocol::AgentForeground {
        self.session
            .agent_work
            .get(agent_instance_id)
            .map(|work| work.foreground)
            .unwrap_or(piko_protocol::AgentForeground::Idle)
    }

    pub fn push_focus(&mut self, mode: AppMode) {
        self.focus_manager.push(mode);
        if !mode.is_surface(SurfaceId::SummaryPrompt) {
            self.clear_filter_for_mode(mode);
        }
        // Runtime agent tree surface owns its own selection chrome.
        self.agent_panel.focus = mode.is_surface(SurfaceId::Agents);
    }

    /// Push a catalog surface onto the focus stack.
    pub fn push_surface(&mut self, surface: SurfaceId) {
        // Modal authority invariant: while a Decide surface is pending, no
        // other surface may take focus — the drawn modal IS the focus owner.
        if let Some(decide) = self.pending_decide()
            && decide != surface
        {
            return;
        }
        self.push_focus(AppMode::from_surface(surface));
    }

    /// Host-priority surface currently in progress: Approval beats Tool
    /// Interaction beats the focused surface. This is the single authority
    /// for what is drawn **and** what owns input.
    pub fn modal_surface(&self) -> Option<SurfaceId> {
        self.pending_decide()
            .or_else(|| self.focus_manager.active_surface())
    }

    /// A blocking Decide surface with a pending request (including the
    /// pending-submission state, where hostd has not resolved yet).
    pub fn pending_decide(&self) -> Option<SurfaceId> {
        if !self.approvals.is_empty() {
            Some(SurfaceId::Approval)
        } else if !self.interactions.is_empty()
            && self.interactions.front().is_some_and(|i| i.surfaced)
        {
            Some(SurfaceId::ToolInteraction)
        } else {
            None
        }
    }

    pub fn pop_focus(&mut self) {
        let popped = self.focus_manager.pop();
        if popped.is_some_and(|mode| mode.is_surface(SurfaceId::History)) {
            self.history = Default::default();
        }
        if !popped.is_some_and(|m| m.is_surface(SurfaceId::SummaryPrompt))
            && let Some(mode) = popped
        {
            self.clear_filter_for_mode(mode);
        }
    }

    pub fn clear_focus(&mut self) {
        self.history = Default::default();
        self.focus_manager.clear_to_chat();
        self.clear_all_filters();
        self.agent_panel.focus = false;
        self.thought_inspector = None;
    }

    pub(crate) fn clear_filter_for_mode(&mut self, mode: AppMode) {
        match mode {
            AppMode::Surface(SurfaceId::Sessions) => self.sessions.filter.clear(),
            AppMode::Surface(SurfaceId::Agents) => self.agent_panel.filter.clear(),
            AppMode::Surface(SurfaceId::Tree) => self.tree.filter.clear(),
            AppMode::Surface(SurfaceId::Models) => self.models.filter.clear(),
            AppMode::Surface(SurfaceId::Settings) => self.settings.filter.clear(),
            AppMode::Surface(SurfaceId::AuthSelector) => self.auth_selector.filter.clear(),
            AppMode::Surface(SurfaceId::History) => {
                self.history.filter.clear();
                self.history.filter_editing = false;
            }
            _ => {}
        }
    }

    pub(crate) fn clear_all_filters(&mut self) {
        self.sessions.filter.clear();
        self.agent_panel.filter.clear();
        self.tree.filter.clear();
        self.models.filter.clear();
        self.thinking.filter.clear();
        self.settings.filter.clear();
        self.auth_selector.filter.clear();
        self.history.filter.clear();
        self.history.filter_editing = false;
    }

    pub(crate) fn active_filter_mut(&mut self) -> Option<&mut String> {
        match self.focus_manager.active_mode() {
            AppMode::Surface(SurfaceId::Sessions) => Some(&mut self.sessions.filter),
            AppMode::Surface(SurfaceId::Agents) => Some(&mut self.agent_panel.filter),
            AppMode::Surface(SurfaceId::Tree) => Some(&mut self.tree.filter),
            AppMode::Surface(SurfaceId::Models) => Some(&mut self.models.filter),
            AppMode::Surface(SurfaceId::Thinking) => Some(&mut self.thinking.filter),
            AppMode::Surface(SurfaceId::Settings) => Some(&mut self.settings.filter),
            AppMode::Surface(SurfaceId::AuthSelector) => match self.auth_selector.state {
                crate::features::auth_selector::AuthSelectorState::Menu => {
                    Some(&mut self.auth_selector.filter)
                }
                crate::features::auth_selector::AuthSelectorState::ApiKeyInput { .. } => None,
            },
            AppMode::Surface(SurfaceId::History) if self.history.filter_editing => {
                Some(&mut self.history.filter)
            }
            _ => None,
        }
    }

    pub(crate) fn settings_snapshot(&self) -> crate::features::settings::SettingsSnapshot {
        crate::features::settings::SettingsSnapshot {
            host: self.host_settings.clone(),
            tui: self.tui_config.clone(),
            thinking_level: self
                .model
                .active_thinking_level
                .clone()
                .or_else(|| self.host_settings.thinking_level.clone()),
            thinking_visible: self.timeline().thinking_visible,
            theme_name: self.theme.name.clone(),
            no_tools: self.initial_options.no_tools || !self.host_settings.all_tools,
        }
    }

    pub(crate) fn apply_settings_action_optimistically(
        &mut self,
        action: &crate::features::settings::SettingsAction,
    ) {
        use crate::features::settings::SettingsAction;
        match action {
            SettingsAction::Thinking(level) => {
                self.model.active_thinking_level = Some((*level).to_string());
                self.host_settings.thinking_level = Some((*level).to_string());
            }
            SettingsAction::HideThinking(hide) => {
                self.timelines.set_thinking_visible(!*hide);
                self.tui_config.hide_thinking_block = *hide;
            }
            SettingsAction::Compaction(v) => self.host_settings.compaction_enabled = *v,
            SettingsAction::CompactionKeep(n) => self.host_settings.compaction_keep = *n,
            SettingsAction::CompactionReserve(n) => self.host_settings.compaction_reserve = *n,
            SettingsAction::CompactionMinGrowthFraction(n) => {
                self.host_settings.compaction_min_growth_fraction = *n;
            }
            SettingsAction::TranscriptMaxToolOutput(n) => {
                self.host_settings.transcript_max_tool_output_tokens = *n;
            }
            SettingsAction::Theme(name) => {
                self.theme = Theme::load(name);
                self.tui_config.theme.name = name.to_string();
            }
            SettingsAction::Transport(t) => {
                self.host_settings.transport = Some((*t).to_string());
            }
            SettingsAction::Retry(v) => self.host_settings.retry_enabled = *v,
            SettingsAction::RetryMaxRetries(n) => self.host_settings.retry_max_retries = *n,
            SettingsAction::RetryBaseDelay(n) => self.host_settings.retry_base_delay_ms = *n,
            SettingsAction::RetryMaxDelay(n) => self.host_settings.retry_max_delay_ms = *n,
            SettingsAction::RetryBudget(n) => self.host_settings.retry_budget_ms = *n,
            SettingsAction::ApprovalTimeout(n) => self.host_settings.approval_timeout_secs = *n,
            SettingsAction::Guardian(v) => self.host_settings.guardian_enabled = *v,
            SettingsAction::GuardianTimeout(n) => self.host_settings.guardian_timeout_secs = *n,
            SettingsAction::GuardianMaxDenials(n) => {
                self.host_settings.guardian_max_consecutive_denials = *n;
            }
            SettingsAction::SafeWorkspaceWrites(v) => {
                self.host_settings.safe_workspace_writes = *v;
            }
            SettingsAction::PermissionProfile(profile) => {
                self.host_settings.permission_profile = profile.clone();
            }
            SettingsAction::Feature(key, enabled) => {
                self.host_settings.features.insert((*key).into(), *enabled);
            }
            SettingsAction::McpConnectTimeout(n) => {
                self.host_settings.mcp_connect_timeout_ms = *n;
            }
            SettingsAction::PromptCache(policy) => {
                self.host_settings.prompt_cache_policy = (*policy).into();
            }
            SettingsAction::Observability(v) => self.host_settings.observability_enabled = *v,
            SettingsAction::ObservabilityEndpoint(ep) => {
                self.host_settings.otel_endpoint = (*ep).to_string();
            }

            SettingsAction::EditorMultiline(value) => {
                self.tui_config.editor.multiline = *value;
            }
            SettingsAction::EditorAutoResize(value) => {
                self.tui_config.editor.auto_resize = *value;
            }
            SettingsAction::EditorMaxLines(value) => {
                self.tui_config.editor.max_lines = *value;
            }
            SettingsAction::EditorHistoryLimit(value) => {
                self.tui_config.editor.history_limit = *value;
            }
            SettingsAction::TreeFilter(value) => {
                self.tui_config.tree.filter_mode = match *value {
                    "no_tools" => crate::config::TreeFilterMode::NoTools,
                    "user_only" => crate::config::TreeFilterMode::UserOnly,
                    "labeled_only" => crate::config::TreeFilterMode::LabeledOnly,
                    "all" => crate::config::TreeFilterMode::All,
                    _ => crate::config::TreeFilterMode::Default,
                };
                self.tree.filter_mode = self.tui_config.tree.filter_mode.into();
            }
            SettingsAction::BottomBarPreset(preset) => {
                use crate::config::bottom_bar::BottomBarItem::*;
                self.tui_config.bottom_bar.items = match *preset {
                    "compact" => vec![Agent, Model, Context],
                    "minimal" => vec![Agent, Model],
                    _ => vec![Agent, Model, Cwd, Context, Cost],
                };
            }
            SettingsAction::EnableAllTools => {
                self.host_settings.all_tools = true;
                self.initial_options.no_tools = false;
            }
            SettingsAction::DisableTools => {
                self.host_settings.all_tools = false;
                self.initial_options.no_tools = true;
            }
        }
        self.editor.configure(&self.tui_config.editor);
    }
}
