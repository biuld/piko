use std::{path::PathBuf, time::Instant};

use anyhow::Result;
use piko_protocol::{
    Command, CommandAck, CommandCatalogItem, ProviderInfo, SessionListScope, SessionTreeEntry,
};

use crate::{
    config::TuiConfig,
    features::{
        approval::ApprovalPanel,
        editor::Editor,
        model_selector::{ModelOption, ModelSelector},
        notifications::{NotificationCenter, NotificationLevel},
        session_list::SessionList,
        settings::{SettingsAction, SettingsPanel},
        timeline::{Timeline, TimelineEntry},
        tool_interaction::ToolInteractionPanel,
        tree::TreePanel,
    },
    host::{HostLine, HostdClient},
    input::focus::FocusManager,
    theme::Theme,
    ui::components::interactive_workflow::InteractiveWorkflow,
};

pub mod command;
mod dispatch;
mod event;
mod slash;

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
    Tree,
    Models,
    Settings,
    Status,
    Help,
    Approval,
    ToolInteraction,
    SummaryPrompt,
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
            AppMode::Tree => Some(Placement::Full),
            AppMode::Status => Some(Placement::Full),
            AppMode::Models => Some(Placement::Partial),
            AppMode::Settings => Some(Placement::Partial),
            AppMode::Approval => Some(Placement::Partial),
            AppMode::ToolInteraction => Some(Placement::Partial),
            AppMode::SummaryPrompt => Some(Placement::Full),
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
    pub session_id: Option<String>,
    pub session_initializing: bool,
    pub pending_turn_text: Option<String>,
    pub requested_session_id: Option<String>,
    pub continue_session: bool,
    pub initial_options: InitialOptions,
    pub active_model_id: Option<String>,
    pub active_provider: Option<String>,
    pub active_thinking_level: Option<String>,
    pub active_turn_id: Option<String>,
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
    pub filter_text: String,
    pub pending_session_list_command_id: Option<String>,
    pub pending_session_open_command_id: Option<String>,

    // panels (each owns its own state + render)
    pub timeline: Timeline,
    pub approvals: ApprovalPanel,
    pub interactions: ToolInteractionPanel,
    pub sessions: SessionList,
    pub models: ModelSelector,
    pub settings: SettingsPanel,
    pub tree: TreePanel,
    pub summary_prompt: Option<InteractiveWorkflow>,

    // notifications
    pub notifications: NotificationCenter,

    // tui config (from hostd settings under `tui` namespace)
    pub tui_config: TuiConfig,

    // active theme (resolved color tokens)
    pub theme: Theme,
}

impl AppState {
    pub fn new(
        cwd: PathBuf,
        requested_session_id: Option<String>,
        continue_session: bool,
        initial_options: InitialOptions,
    ) -> Self {
        Self {
            cwd,
            session_id: None,
            session_initializing: requested_session_id.is_some() || continue_session,
            pending_turn_text: None,
            requested_session_id,
            continue_session,
            active_model_id: initial_options.model_id.clone(),
            active_provider: initial_options.provider.clone(),
            active_thinking_level: initial_options.thinking_level.clone(),
            initial_options,
            active_turn_id: None,
            mode: AppMode::Chat,
            focus_manager: FocusManager::new(),
            quit: false,
            last_tick: Instant::now(),
            editor: Editor::default(),
            command_catalog: Vec::new(),
            status: "starting hostd".to_string(),
            queue_status: QueueStatus::default(),
            spinner_frame: 0,
            filter_text: String::new(),
            pending_session_list_command_id: None,
            pending_session_open_command_id: None,
            timeline: Timeline::new(),
            approvals: ApprovalPanel::new(),
            interactions: ToolInteractionPanel::new(),
            sessions: SessionList::new(),
            models: ModelSelector::new(),
            settings: SettingsPanel::new(),
            tree: TreePanel::new(),
            summary_prompt: None,
            notifications: NotificationCenter::default(),
            tui_config: TuiConfig::default(),
            theme: Theme::dark(),
        }
    }

    // ── accessors ─────────────────────────────────────────────────────────────

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn active_turn_id(&self) -> Option<&str> {
        self.active_turn_id.as_deref()
    }

    pub fn cwd(&self) -> PathBuf {
        self.cwd.clone()
    }

    pub fn push_focus(&mut self, mode: AppMode) {
        self.focus_manager.push(mode);
        self.mode = self.focus_manager.active_mode();
        if mode != AppMode::SummaryPrompt {
            self.filter_text.clear();
        }
    }

    pub fn pop_focus(&mut self) {
        let popped = self.focus_manager.pop();
        self.mode = self.focus_manager.active_mode();
        if popped != Some(AppMode::SummaryPrompt) {
            self.filter_text.clear();
        }
    }

    pub fn clear_focus(&mut self) {
        self.focus_manager.clear_to_chat();
        self.mode = self.focus_manager.active_mode();
        self.filter_text.clear();
    }

    // ── bootstrap ─────────────────────────────────────────────────────────────

    pub fn bootstrap(&mut self, host: &mut HostdClient) -> Result<()> {
        // Request TUI-specific settings from hostd
        host.send(Command::ConfigGet {
            command_id: command_id(),
            namespace: "tui".to_string(),
        })?;
        host.send(Command::CommandCatalogGet {
            command_id: command_id(),
        })?;

        self.bootstrap_config(host)?;
        if let Some(session_id) = self.requested_session_id.clone() {
            host.send(Command::SessionOpen {
                command_id: command_id(),
                session_id,
                session_path: None,
            })?;
            self.status = "opening session".to_string();
        } else if self.continue_session {
            host.send(Command::SessionList {
                command_id: command_id(),
                scope: SessionListScope::All,
                cwd: None,
            })?;
            self.status = "loading sessions".to_string();
        } else {
            // Wait for the user to submit a turn before creating a session
            self.status = "ready".to_string();
        }
        Ok(())
    }

    fn bootstrap_config(&mut self, host: &mut HostdClient) -> Result<()> {
        if let (Some(provider), Some(api_key)) = (
            self.initial_options.provider.clone(),
            self.initial_options.api_key.clone(),
        ) {
            host.send(Command::AuthSetApiKey {
                command_id: command_id(),
                provider,
                api_key,
            })?;
        }

        host.send(Command::ConfigSet {
            command_id: command_id(),
            default_provider: self.initial_options.provider.clone(),
            default_model: self.initial_options.model_id.clone(),
            default_thinking_level: self.initial_options.thinking_level.clone(),
            active_tools: self.initial_options.no_tools.then(Vec::new),
            theme: None,
            hide_thinking_block: None,
            transport: None,
            compaction_enabled: None,
            compaction_reserve_tokens: None,
            compaction_keep_recent_tokens: None,
        })?;

        Ok(())
    }

    // ── host line handling ────────────────────────────────────────────────────

    pub fn handle_host_line(&mut self, host: &mut HostdClient, line: HostLine) {
        match line {
            HostLine::Ack(CommandAck::CommandAccepted { command_id }) => {
                self.status = format!("accepted {command_id}");
                self.notify(NotificationLevel::Info, format!("accepted {command_id}"));
            }
            HostLine::Ack(CommandAck::CommandRejected { command_id, reason }) => {
                self.status = format!("rejected {command_id}");
                if self.pending_session_list_command_id.as_deref() == Some(command_id.as_str())
                    || self.pending_session_open_command_id.as_deref() == Some(command_id.as_str())
                {
                    self.sessions.loading = false;
                    self.sessions.error = Some(reason.clone());
                    if self.pending_session_list_command_id.as_deref() == Some(command_id.as_str())
                    {
                        self.pending_session_list_command_id = None;
                    }
                    if self.pending_session_open_command_id.as_deref() == Some(command_id.as_str())
                    {
                        self.pending_session_open_command_id = None;
                    }
                }
                self.notify(
                    NotificationLevel::Error,
                    format!("rejected {command_id}: {reason}"),
                );
                self.push(TimelineEntry::Error(reason));
            }
            HostLine::Event(event) => self.apply_event(Some(host), *event),
            HostLine::DecodeError(err) => {
                self.notify(NotificationLevel::Error, err.clone());
                self.push(TimelineEntry::Error(err));
            }
            HostLine::Closed => {
                self.status = "hostd closed stdout".to_string();
                self.notify(NotificationLevel::Warning, "hostd closed stdout");
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

    while let Some(id) = curr_id {
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
    empty_config_set_with(|c| match action {
        SettingsAction::Thinking(level) => {
            if let Command::ConfigSet {
                default_thinking_level,
                ..
            } = c
            {
                *default_thinking_level = Some(level.to_string());
            }
        }
        SettingsAction::HideThinking(value) => {
            if let Command::ConfigSet {
                hide_thinking_block,
                ..
            } = c
            {
                *hide_thinking_block = Some(value);
            }
        }
        SettingsAction::Compaction(value) => {
            if let Command::ConfigSet {
                compaction_enabled, ..
            } = c
            {
                *compaction_enabled = Some(value);
            }
        }
        SettingsAction::Theme(value) => {
            if let Command::ConfigSet { theme, .. } = c {
                *theme = Some(value.to_string());
            }
        }
        SettingsAction::Transport(value) => {
            if let Command::ConfigSet { transport, .. } = c {
                *transport = Some(value.to_string());
            }
        }
        SettingsAction::DisableTools => {
            if let Command::ConfigSet { active_tools, .. } = c {
                *active_tools = Some(Vec::new());
            }
        }
    })
}

fn empty_config_set_with(f: impl FnOnce(&mut Command)) -> Command {
    let mut c = Command::ConfigSet {
        command_id: command_id(),
        default_provider: None,
        default_model: None,
        default_thinking_level: None,
        active_tools: None,
        theme: None,
        hide_thinking_block: None,
        transport: None,
        compaction_enabled: None,
        compaction_reserve_tokens: None,
        compaction_keep_recent_tokens: None,
    };
    f(&mut c);
    c
}
