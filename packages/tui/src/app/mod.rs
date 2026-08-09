use std::{collections::HashMap, path::PathBuf, time::Instant};

use piko_protocol::{Command, ProviderInfo, SessionTreeEntry};

use crate::{
    config::TuiConfig,
    features::{
        agent_status::AgentPanelState,
        approval::ApprovalPanel,
        auth_selector::AuthSelector,
        editor::Editor,
        model_selector::{ModelOption, ModelSelector},
        notifications::NotificationCenter,
        session_list::SessionList,
        settings::{HostRuntimeSettings, SettingsAction, SettingsPanel},
        thinking::ThinkingSelector,
        timeline::Timeline,
        tool_interaction::ToolInteractionPanel,
        tree::TreePanel,
    },
    input::focus::FocusManager,
    theme::Theme,
    ui::components::{interactive_workflow::InteractiveWorkflow, text_box::TextBox},
};

mod bootstrap;
pub mod command;
pub mod confirm;
mod dispatch;
pub mod effect;
mod event;
mod pending;
mod runtime;
mod session_ops;
mod session_view;
mod slash;
mod turn;

#[cfg(test)]
mod tests;

// ── public types ──────────────────────────────────────────────────────────────

// Product focus / surface catalog (lives under navigation; engine is generic).
pub use crate::navigation::{AppMode, Region, SurfaceId};

/// Stable element ids for pointer hit-testing inside a surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum HitId {
    /// The conversation stream (wheel scroll; click no-op).
    Stream,
    /// The composer (click → focus + place cursor).
    Composer,
    /// The transient notice row (click → dismiss).
    Notice,
    /// One completion suggestion row (click → accept it).
    Suggest(usize),
    /// One visible Timeline tool component (click → toggle this block only).
    TimelineTool(usize),
    /// One source row owned by a selectable surface.
    Row(usize),
    /// An editable field owned by a surface.
    TextInput,
    /// A scrollable content viewport owned by a surface.
    Content,
    /// A pane close affordance.
    Close,
    /// One option in a pane title mode strip.
    Mode(usize),
    /// A question tab in a multi-question workflow.
    Tab(usize),
    /// One choice row of the active question.
    Choice { question: usize, choice: usize },
    /// The Submit step (confirm row or the Submit tab).
    Submit,
}

/// Tool status shared between surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
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
    /// Last pointer hover target (region + element), resolved from the hit
    /// map on `Moved` and consumed by product rendering as soft feedback.
    pub hovered: Option<(Region, Option<HitId>)>,

    // core input
    pub editor: Editor,
    pub command_catalog: Vec<command::TuiCommandEntry>,

    // session-level status
    pub status: String,
    pub queue_status: QueueStatus,
    pub spinner_frame: usize,

    // panels (each owns its own state + render)
    pub timeline: Timeline,
    pub agent_timelines: HashMap<String, Timeline>,
    /// Session-scoped durable Timeline entries merged into every agent view.
    pub session_timeline_entries: Vec<(piko_protocol::SessionTreeEntry, u64)>,
    pub approvals: ApprovalPanel,
    pub mcp: crate::features::mcp::McpPanel,
    pub processes: crate::features::processes::ProcessPanel,
    pub diagnostics: crate::features::diagnostics::DiagnosticsPanel,
    /// Last known turn id for `/diff` when no turn is actively running.
    pub last_turn_id: Option<String>,
    /// Last push/result turn diff for offline re-open via `/diff`.
    pub last_turn_diff: Option<piko_protocol::TurnDiffEvent>,
    pub interactions: ToolInteractionPanel,
    pub sessions: SessionList,
    pub models: ModelSelector,
    pub thinking: ThinkingSelector,
    pub settings: SettingsPanel,
    pub tree: TreePanel,
    pub summary_prompt: Option<InteractiveWorkflow>,
    pub auth_selector: AuthSelector,
    /// True while the Tree surface is choosing a branch point for `/fork`.
    pub tree_fork_mode: bool,

    // agent panel (multi-agent switching)
    pub agent_panel: AgentPanelState,

    // notifications
    pub notifications: NotificationCenter,

    // tui config (from hostd settings under `tui` namespace)
    pub tui_config: TuiConfig,

    // host runtime settings mirror (ConfigGet namespace "host")
    pub host_settings: HostRuntimeSettings,

    // active theme (resolved color tokens)
    pub theme: Theme,
}

/// In-flight turn tracked for F-22 foreground projection (status-aware).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveTurnUi {
    pub turn_id: String,
    pub status: piko_protocol::TurnStatus,
}

#[derive(Clone, Debug, Default)]
pub struct SessionUiState {
    /// Session whose authoritative view has been reconciled and is live.
    pub id: Option<String>,
    /// Session currently being created/opened; never used as a chat target.
    pub opening_id: Option<String>,
    /// Previously live session to restore if a switch fails.
    pub previous_live_id: Option<String>,
    pub initializing: bool,
    pub shell_ready: bool,
    pub pending_turn_text: Option<String>,
    pub requested_id: Option<String>,
    pub continue_requested: bool,
    /// agent_instance_id → active turn (id + status) for F-22 foreground projection.
    pub active_turns: HashMap<String, ActiveTurnUi>,
    pub pending: pending::PendingCommands,
    /// Session-wide token/cost ledger projected from hostd (F-15 / D-29).
    pub cumulative_usage: Option<piko_protocol::messages::Usage>,
    /// Last known prompt-side tokens (input + cache_read) for context fill.
    pub last_context_tokens: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct ModelUiState {
    pub active_model_id: Option<String>,
    pub active_provider: Option<String>,
    pub active_thinking_level: Option<String>,
    /// Host-pushed window from `ModelEvent::ConfigChanged` (F-22 / D-34).
    pub host_context_window: Option<u64>,
    pub providers: Vec<ProviderInfo>,
}

impl ModelUiState {
    /// Context window for the active model: host field first, then catalog.
    pub fn active_context_window(&self) -> Option<u64> {
        if let Some(window) = self.host_context_window.filter(|w| *w > 0) {
            return Some(window);
        }
        let model_id = self.active_model_id.as_deref()?;
        for provider in &self.providers {
            for model in &provider.models {
                let full = format!("{}/{}", provider.provider, model.id);
                let provider_matches = self
                    .active_provider
                    .as_deref()
                    .is_none_or(|p| p == provider.provider);
                let matches = model_id == full
                    || (provider_matches && model_id == model.id)
                    || model_id == model.name;
                if matches && model.context_window > 0 {
                    return Some(model.context_window);
                }
            }
        }
        None
    }
}

mod impls;

pub fn command_id() -> String {
    format!("tui-{}", uuid::Uuid::new_v4())
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

pub(crate) fn config_command_for_setting(action: SettingsAction) -> Command {
    let patch = match action {
        SettingsAction::Thinking(level) => {
            serde_json::json!({
                "default-thinking-level": level
            })
        }
        SettingsAction::HideThinking(value) => {
            // TUI-only presentation; lives under `[tui]`.
            serde_json::json!({
                "tui": { "hide_thinking_block": value }
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
        SettingsAction::CompactionMinGrowthFraction(value) => {
            serde_json::json!({
                "compaction": { "min-growth-fraction": value }
            })
        }
        SettingsAction::TranscriptMaxToolOutput(value) => {
            serde_json::json!({
                "transcript": { "max-tool-output-tokens": value }
            })
        }
        SettingsAction::Theme(value) => {
            // Theme is TUI presentation; lives under `[tui].theme.name`.
            serde_json::json!({
                "tui": { "theme": { "name": value } }
            })
        }
        SettingsAction::Transport(value) => {
            serde_json::json!({
                "transport": value
            })
        }
        SettingsAction::Retry(value) => {
            serde_json::json!({
                "retry": {
                    "enabled": value
                }
            })
        }
        SettingsAction::RetryMaxRetries(value) => {
            serde_json::json!({ "retry": { "max-retries": value } })
        }
        SettingsAction::RetryBaseDelay(value) => {
            serde_json::json!({ "retry": { "base-delay-ms": value } })
        }
        SettingsAction::RetryMaxDelay(value) => {
            serde_json::json!({ "retry": { "max-delay-ms": value } })
        }
        SettingsAction::RetryBudget(value) => {
            serde_json::json!({ "retry": { "budget-ms": value } })
        }
        SettingsAction::ApprovalTimeout(value) => {
            serde_json::json!({ "approvals": { "timeout-secs": value } })
        }
        SettingsAction::Guardian(value) => {
            serde_json::json!({ "guardian": { "enabled": value } })
        }
        SettingsAction::GuardianTimeout(value) => {
            serde_json::json!({ "guardian": { "timeout-secs": value } })
        }
        SettingsAction::GuardianMaxDenials(value) => {
            serde_json::json!({ "guardian": { "max-consecutive-denials": value } })
        }
        SettingsAction::SafeWorkspaceWrites(value) => {
            serde_json::json!({ "safety": { "auto-approve-workspace-writes": value } })
        }
        SettingsAction::PermissionProfile(value) => {
            serde_json::json!({ "permissions": { "profile": value } })
        }
        SettingsAction::Feature(key, value) => {
            serde_json::json!({ "features": { (key): value } })
        }
        SettingsAction::McpConnectTimeout(value) => {
            serde_json::json!({ "mcp": { "connect-timeout-ms": value } })
        }
        SettingsAction::PromptCache(value) => {
            serde_json::json!({ "prompt": { "cache-policy": value } })
        }
        SettingsAction::Observability(value) => {
            serde_json::json!({
                "observability": {
                    "enabled": value
                }
            })
        }
        SettingsAction::ObservabilityEndpoint(endpoint) => {
            serde_json::json!({
                "observability": {
                    "otel-endpoint": endpoint
                }
            })
        }
        SettingsAction::EditorMultiline(value) => {
            serde_json::json!({ "tui": { "editor": { "multiline": value } } })
        }
        SettingsAction::EditorAutoResize(value) => {
            serde_json::json!({ "tui": { "editor": { "autoResize": value } } })
        }
        SettingsAction::EditorMaxLines(value) => {
            serde_json::json!({ "tui": { "editor": { "maxLines": value } } })
        }
        SettingsAction::EditorHistoryLimit(value) => {
            serde_json::json!({ "tui": { "editor": { "historyLimit": value } } })
        }
        SettingsAction::TreeFilter(value) => {
            serde_json::json!({ "tui": { "tree": { "filter_mode": value } } })
        }
        SettingsAction::BottomBarPreset(value) => {
            let items = match value {
                "compact" => vec!["agent", "model", "context"],
                "minimal" => vec!["agent", "model"],
                _ => vec!["agent", "model", "cwd", "context", "cost"],
            };
            serde_json::json!({ "tui": { "bottom_bar": { "items": items } } })
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
