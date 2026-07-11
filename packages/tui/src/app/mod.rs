use std::{collections::HashMap, path::PathBuf, time::Instant};

use piko_protocol::{
    Command, CommandCatalogItem, ProviderInfo, SessionListScope, SessionTreeEntry,
};

use crate::{
    config::TuiConfig,
    features::{
        agent_status::AgentPanelState,
        approval::ApprovalPanel,
        auth_selector::AuthSelector,
        editor::Editor,
        model_selector::{ModelOption, ModelSelector},
        notifications::{NotificationCenter, NotificationLevel},
        session_list::SessionList,
        settings::{SettingsAction, SettingsPanel},
        timeline::{Timeline, TimelineEntry},
        tool_interaction::ToolInteractionPanel,
        tree::TreePanel,
    },
    host::HostLine,
    input::focus::FocusManager,
    theme::Theme,
    ui::components::{interactive_workflow::InteractiveWorkflow, text_box::TextBox},
};

pub mod command;
pub mod confirm;
mod dispatch;
pub mod effect;
mod event;
mod palette;
mod session_ops;
mod slash;
mod turn;

#[cfg(test)]
mod tests;

// ── public types ──────────────────────────────────────────────────────────────

/// Tool status shared between surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolStatus {
    Running,
    Completed,
    Failed,
}

/// Which overlay / mode is currently shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppMode {
    Chat,
    Sessions,
    AgentList,
    Tree,
    Models,
    Settings,
    Status,
    Help,
    Approval,
    ToolInteraction,
    SummaryPrompt,
    AuthSelector,
    AgentPanel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Placement {
    Full,
    Partial,
}

impl AppMode {
    pub fn placement(&self) -> Option<Placement> {
        match self {
            AppMode::Chat => None,
            AppMode::Help => Some(Placement::Full),
            AppMode::Sessions => Some(Placement::Full),
            AppMode::AgentList => Some(Placement::Full),
            AppMode::Tree => Some(Placement::Full),
            AppMode::Status => Some(Placement::Full),
            AppMode::Models => Some(Placement::Partial),
            AppMode::Settings => Some(Placement::Partial),
            AppMode::Approval => Some(Placement::Partial),
            AppMode::ToolInteraction => Some(Placement::Partial),
            AppMode::SummaryPrompt => Some(Placement::Full),
            AppMode::AuthSelector => Some(Placement::Partial),
            AppMode::AgentPanel => None,
        }
    }
}

#[derive(Clone, Default)]
pub struct QueueStatus {
    pub steer_count: u32,
    pub follow_up_count: u32,
    pub next_turn_count: u32,
    pub steer_preview: Option<String>,
    pub follow_up_preview: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct InitialOptions {
    pub model_id: Option<String>,
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub thinking_level: Option<String>,
    pub session_name: Option<String>,
    pub no_tools: bool,
}

// ── AppState ──────────────────────────────────────────────────────────────────

/// Central application state.  Each surface owns its own data; AppState wires
/// them together and handles the hostd protocol.
pub struct AppState {
    // identity / routing
    pub cwd: PathBuf,
    pub initial_options: InitialOptions,
    pub session: SessionUiState,
    pub model: ModelUiState,
    pub mode: AppMode,
    pub focus_manager: FocusManager,
    pub quit: bool,
    pub last_tick: Instant,

    // core input
    pub editor: Editor,
    pub command_catalog: Vec<CommandCatalogItem>,

    // session-level status
    pub status: String,
    pub queue_status: QueueStatus,
    pub spinner_frame: usize,

    // panels (each owns its own state + render)
    pub timeline: Timeline,
    pub task_timelines: HashMap<String, Timeline>,
    pub approvals: ApprovalPanel,
    pub interactions: ToolInteractionPanel,
    pub sessions: SessionList,
    pub agents: crate::features::agent_list::AgentList,
    pub models: ModelSelector,
    pub settings: SettingsPanel,
    pub tree: TreePanel,
    pub summary_prompt: Option<InteractiveWorkflow>,
    pub auth_selector: AuthSelector,

    // agent panel (multi-agent switching)
    pub agent_panel: AgentPanelState,

    // notifications
    pub notifications: NotificationCenter,

    // tui config (from hostd settings under `tui` namespace)
    pub tui_config: TuiConfig,

    // active theme (resolved color tokens)
    pub theme: Theme,
}

#[derive(Clone, Debug, Default)]
pub struct SessionUiState {
    pub id: Option<String>,
    pub initializing: bool,
    pub pending_turn_text: Option<String>,
    pub requested_id: Option<String>,
    pub continue_requested: bool,
    pub active_turn_id: Option<String>,
    pub pending_list_command_id: Option<String>,
    pub pending_open_command_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ModelUiState {
    pub active_model_id: Option<String>,
    pub active_provider: Option<String>,
    pub active_thinking_level: Option<String>,
    pub providers: Vec<ProviderInfo>,
}

impl AppState {
    pub fn new(
        cwd: PathBuf,
        requested_session_id: Option<String>,
        continue_session: bool,
        initial_options: InitialOptions,
    ) -> Self {
        let session = SessionUiState {
            initializing: requested_session_id.is_some() || continue_session,
            requested_id: requested_session_id,
            continue_requested: continue_session,
            ..Default::default()
        };
        let model = ModelUiState {
            active_model_id: initial_options.model_id.clone(),
            active_provider: initial_options.provider.clone(),
            active_thinking_level: initial_options.thinking_level.clone(),
            providers: Vec::new(),
        };
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
            task_timelines: HashMap::new(),
            approvals: ApprovalPanel::new(),
            interactions: ToolInteractionPanel::new(),
            sessions: SessionList::new(),
            agents: crate::features::agent_list::AgentList::new(),
            models: ModelSelector::new(),
            settings: SettingsPanel::new(),
            tree: TreePanel::new(),
            summary_prompt: None,
            auth_selector: AuthSelector::new(&[]),
            agent_panel: AgentPanelState::default(),
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
        self.session.active_turn_id.as_deref()
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

    pub fn update(&mut self, msg: effect::Msg) -> Vec<effect::Effect> {
        match msg {
            effect::Msg::Action(action) => self.dispatch(action),
            effect::Msg::HostLine(line) => self.handle_host_line(line),
            effect::Msg::Tick => {
                self.last_tick = Instant::now();
                self.spinner_frame = self.spinner_frame.wrapping_add(1);
                self.timeline.viewport.apply_metrics();
                Vec::new()
            }
        }
    }

    // ── bootstrap ─────────────────────────────────────────────────────────────

    pub fn bootstrap(&mut self) -> Vec<effect::Effect> {
        let mut effects = Vec::new();
        // Request TUI-specific settings from hostd
        effects.push(effect::Effect::send(Command::ConfigGet {
            command_id: command_id(),
            namespace: "tui".to_string(),
        }));
        effects.push(effect::Effect::send(Command::CommandCatalogGet {
            command_id: command_id(),
        }));

        effects.extend(self.bootstrap_config());
        if let Some(session_id) = self.session.requested_id.clone() {
            effects.push(effect::Effect::send(Command::SessionOpen {
                command_id: command_id(),
                session_id,
                session_path: None,
            }));
            self.status = "opening session".to_string();
        } else if self.session.continue_requested {
            effects.push(effect::Effect::send(Command::SessionList {
                command_id: command_id(),
                scope: SessionListScope::All,
                cwd: None,
            }));
            self.status = "loading sessions".to_string();
        } else {
            // Wait for the user to submit a turn before creating a session
            self.status = "ready".to_string();
        }
        effects
    }

    fn bootstrap_config(&mut self) -> Vec<effect::Effect> {
        let mut effects = Vec::new();
        if let (Some(provider), Some(api_key)) = (
            self.initial_options.provider.clone(),
            self.initial_options.api_key.clone(),
        ) {
            effects.push(effect::Effect::send(Command::AuthSetApiKey {
                command_id: command_id(),
                provider,
                api_key,
            }));
        }

        let mut patch = serde_json::Map::new();
        if let Some(ref provider) = self.initial_options.provider {
            patch.insert("default-provider".to_string(), serde_json::json!(provider));
        }
        if let Some(ref model_id) = self.initial_options.model_id {
            patch.insert("default-model".to_string(), serde_json::json!(model_id));
        }
        if let Some(ref thinking_level) = self.initial_options.thinking_level {
            patch.insert(
                "default-thinking-level".to_string(),
                serde_json::json!(thinking_level),
            );
        }
        if self.initial_options.no_tools {
            patch.insert(
                "active-tool-names".to_string(),
                serde_json::json!(Vec::<String>::new()),
            );
        }

        effects.push(effect::Effect::send(Command::ConfigUpdate {
            command_id: command_id(),
            patch: serde_json::Value::Object(patch),
        }));

        effects
    }

    // ── host line handling ────────────────────────────────────────────────────

    pub fn handle_host_line(&mut self, line: HostLine) -> Vec<effect::Effect> {
        match line {
            HostLine::Message(message) => match *message {
                piko_protocol::ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(piko_protocol::CommandResult::Empty),
                } => {
                    self.status = format!("accepted {command_id}");
                    self.notify(NotificationLevel::Info, format!("accepted {command_id}"));
                    Vec::new()
                }
                piko_protocol::ServerMessage::CommandResponse {
                    command_id,
                    result: Err(reason),
                } => {
                    self.status = format!("rejected {command_id}");
                    if self.session.pending_list_command_id.as_deref() == Some(command_id.as_str())
                        || self.session.pending_open_command_id.as_deref()
                            == Some(command_id.as_str())
                    {
                        self.sessions.loading = false;
                        self.sessions.error = Some(reason.clone());
                        if self.session.pending_list_command_id.as_deref()
                            == Some(command_id.as_str())
                        {
                            self.session.pending_list_command_id = None;
                        }
                        if self.session.pending_open_command_id.as_deref()
                            == Some(command_id.as_str())
                        {
                            self.session.pending_open_command_id = None;
                        }
                    }
                    self.notify(
                        NotificationLevel::Error,
                        format!("rejected {command_id}: {reason}"),
                    );
                    self.push(TimelineEntry::Error(reason));
                    Vec::new()
                }
                message => self.apply_event(message),
            },
            HostLine::DecodeError(err) => {
                self.notify(NotificationLevel::Error, err.clone());
                self.push(TimelineEntry::Error(err));
                Vec::new()
            }
            HostLine::Closed => {
                self.status = "hostd closed stdout".to_string();
                self.notify(NotificationLevel::Warning, "hostd closed stdout");
                Vec::new()
            }
        }
    }

    // ── tiny helpers ──────────────────────────────────────────────────────────

    pub fn push(&mut self, entry: TimelineEntry) {
        self.timeline.push(entry);
    }

    pub fn push_error(&mut self, message: String) {
        self.notify(NotificationLevel::Error, message.clone());
        self.push(TimelineEntry::Error(message));
    }

    pub fn notify(&mut self, level: NotificationLevel, message: impl Into<String>) {
        self.notifications.push(level, message);
    }
}

// ── module-level helpers ──────────────────────────────────────────────────────

pub fn command_id() -> String {
    format!("tui-{}", uuid::Uuid::new_v4())
}

pub fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

pub fn get_active_branch_entries(
    entries: &[SessionTreeEntry],
    current_leaf_id: Option<&str>,
) -> Vec<SessionTreeEntry> {
    let Some(leaf_id) = current_leaf_id else {
        return entries.to_vec();
    };
    let mut by_id = std::collections::HashMap::new();
    for entry in entries {
        by_id.insert(entry.id(), entry);
    }

    let mut path = Vec::new();
    let mut curr_id = Some(leaf_id.to_string());
    let mut visited = std::collections::HashSet::new();

    while let Some(id) = curr_id {
        if !visited.insert(id.clone()) {
            break; // cycle detected (e.g. id == parentId)
        }
        if let Some(entry) = by_id.get(id.as_str()) {
            path.push((*entry).clone());
            curr_id = entry.parent_id().map(|s| s.to_string());
        } else {
            break;
        }
    }

    path.reverse();
    path
}

fn flatten_models(providers: Vec<ProviderInfo>) -> Vec<ModelOption> {
    providers
        .into_iter()
        .flat_map(|provider| {
            provider.models.into_iter().map(move |model| ModelOption {
                provider: provider.provider.clone(),
                id: model.id,
                name: model.name,
                has_auth: provider.has_auth,
            })
        })
        .collect()
}

fn config_command_for_setting(action: SettingsAction) -> Command {
    let patch = match action {
        SettingsAction::Thinking(level) => {
            serde_json::json!({
                "default-thinking-level": level
            })
        }
        SettingsAction::HideThinking(value) => {
            serde_json::json!({
                "hide-thinking-block": value
            })
        }
        SettingsAction::Compaction(value) => {
            serde_json::json!({
                "compaction": {
                    "enabled": value
                }
            })
        }
        SettingsAction::CompactionKeep(value) => {
            serde_json::json!({
                "compaction": {
                    "keep-recent-tokens": value
                }
            })
        }
        SettingsAction::CompactionReserve(value) => {
            serde_json::json!({
                "compaction": {
                    "reserve-tokens": value
                }
            })
        }
        SettingsAction::Theme(value) => {
            serde_json::json!({
                "theme": value
            })
        }
        SettingsAction::Transport(value) => {
            serde_json::json!({
                "transport": value
            })
        }
        SettingsAction::Sandbox(value) => {
            serde_json::json!({
                "sandbox": {
                    "enabled": value
                }
            })
        }
        SettingsAction::Retry(value) => {
            serde_json::json!({
                "retry": {
                    "enabled": value
                }
            })
        }
        SettingsAction::EnableAllTools => {
            serde_json::json!({
                "active-tool-names": serde_json::Value::Null
            })
        }
        SettingsAction::DisableTools => {
            serde_json::json!({
                "active-tool-names": []
            })
        }
    };
    Command::ConfigUpdate {
        command_id: command_id(),
        patch,
    }
}
