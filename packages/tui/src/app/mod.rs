use std::{path::PathBuf, time::Instant};

use anyhow::Result;
use piko_protocol::{Command, CommandAck, ProviderInfo, SessionTreeEntry};

use crate::{
    host::{HostLine, HostdClient},
    input::{completion::Completion, editor::Editor, focus::FocusManager},
    notification::{NotificationCenter, NotificationLevel},
    surfaces::{
        approval::ApprovalOverlay,
        commands::CommandsOverlay,
        models::{ModelOption, ModelsOverlay},
        sessions::SessionsOverlay,
        settings::{SettingsAction, SettingsOverlay},
        timeline::{TimelineEntry, TimelineView},
        tree::TreeOverlay,
    },
};

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
    Commands,
    Sessions,
    Tree,
    Models,
    Settings,
    Status,
    Help,
    Approval,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfacePlacement {
    Full,
    Partial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceInputPolicy {
    Capture,
    Passive,
}

impl AppMode {
    pub fn placement(&self) -> Option<SurfacePlacement> {
        match self {
            AppMode::Chat => None,
            AppMode::Help => Some(SurfacePlacement::Full),
            AppMode::Sessions => Some(SurfacePlacement::Full),
            AppMode::Tree => Some(SurfacePlacement::Full),
            AppMode::Status => Some(SurfacePlacement::Full),
            AppMode::Commands => Some(SurfacePlacement::Partial),
            AppMode::Models => Some(SurfacePlacement::Partial),
            AppMode::Settings => Some(SurfacePlacement::Partial),
            AppMode::Approval => Some(SurfacePlacement::Partial),
        }
    }

    pub fn input_policy(&self) -> SurfaceInputPolicy {
        match self {
            AppMode::Chat => SurfaceInputPolicy::Passive,
            _ => SurfaceInputPolicy::Capture,
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
    pub requested_session_id: Option<String>,
    pub continue_session: bool,
    pub initial_options: InitialOptions,
    pub active_turn_id: Option<String>,
    pub mode: AppMode,
    pub focus_manager: FocusManager,
    pub quit: bool,
    pub last_tick: Instant,

    // core input
    pub editor: Editor,
    pub completions: Vec<Completion>,
    pub selected_completion: usize,

    // session-level status
    pub status: String,
    pub queue_status: QueueStatus,
    pub spinner_frame: usize,

    // surfaces (each owns its own state + render)
    pub timeline: TimelineView,
    pub approvals: ApprovalOverlay,
    pub commands: CommandsOverlay,
    pub sessions: SessionsOverlay,
    pub models: ModelsOverlay,
    pub settings: SettingsOverlay,
    pub tree: TreeOverlay,

    // notifications
    pub notifications: NotificationCenter,
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
            requested_session_id,
            continue_session,
            initial_options,
            active_turn_id: None,
            mode: AppMode::Chat,
            focus_manager: FocusManager::new(),
            quit: false,
            last_tick: Instant::now(),
            editor: Editor::default(),
            completions: Vec::new(),
            selected_completion: 0,
            status: "starting hostd".to_string(),
            queue_status: QueueStatus::default(),
            spinner_frame: 0,
            timeline: TimelineView::new(),
            approvals: ApprovalOverlay::new(),
            commands: CommandsOverlay::new(),
            sessions: SessionsOverlay::new(),
            models: ModelsOverlay::new(),
            settings: SettingsOverlay::new(),
            tree: TreeOverlay::new(),
            notifications: NotificationCenter::default(),
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
    }

    pub fn pop_focus(&mut self) {
        self.focus_manager.pop();
        self.mode = self.focus_manager.active_mode();
    }

    pub fn clear_focus(&mut self) {
        self.focus_manager.clear_to_chat();
        self.mode = self.focus_manager.active_mode();
    }

    // ── bootstrap ─────────────────────────────────────────────────────────────

    pub fn bootstrap(&mut self, host: &mut HostdClient) -> Result<()> {
        self.bootstrap_config(host)?;
        if let Some(session_id) = self.requested_session_id.clone() {
            host.send(Command::SessionOpen {
                command_id: command_id(),
                session_id,
            })?;
            self.status = "opening session".to_string();
        } else if self.continue_session {
            host.send(Command::SessionList {
                command_id: command_id(),
            })?;
            self.status = "loading sessions".to_string();
        } else {
            host.send(Command::SessionCreate {
                command_id: command_id(),
                cwd: self.cwd.to_string_lossy().into_owned(),
            })?;
            self.status = "creating session".to_string();
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
        let has_config = self.initial_options.provider.is_some()
            || self.initial_options.model_id.is_some()
            || self.initial_options.thinking_level.is_some()
            || self.initial_options.no_tools;
        if has_config {
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
        }
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
                self.notify(
                    NotificationLevel::Error,
                    format!("rejected {command_id}: {reason}"),
                );
                self.push(TimelineEntry::Error(reason));
            }
            HostLine::Event(event) => self.apply_event(Some(host), event),
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
