use std::{collections::HashMap, path::PathBuf, time::Instant};

use piko_protocol::ProviderInfo;

use crate::{
    config::TuiConfig,
    features::{
        agent_status::AgentPanelState,
        approval::ApprovalPanel,
        auth_selector::AuthSelector,
        editor::Editor,
        history::HistoryPanel,
        model_selector::{ModelOption, ModelSelector},
        notifications::NotificationCenter,
        session_list::SessionList,
        settings::{HostRuntimeSettings, SettingsPanel},
        thinking::ThinkingSelector,
        thought_inspector::ThoughtInspectorState,
        timeline::{Timeline, TimelineStore},
        todos::TodoListsState,
        tool_interaction::ToolInteractionPanel,
        tree::TreePanel,
    },
    input::{binding::BindingRegistry, focus::FocusManager},
    theme::Theme,
    ui::components::{choice_workflow::ChoiceWorkflow, text_box::TextBox},
};

mod agent_control;
mod bootstrap;
pub mod command;
pub mod confirm;
mod dispatch;
pub mod effect;
mod event;
mod helpers;
mod history;
mod pending;
mod runtime;
mod session_ops;
mod session_view;
mod slash;
mod submit;

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
    /// Copy one notification's complete original message.
    NotificationCopy(u64),
    /// One completion suggestion row (click → accept it).
    Suggest(usize),
    /// One visible Timeline tool component (click → toggle this block only).
    /// Payload is the Timeline-interned stable tool identity, not a component
    /// slot: it survives rebuilds and never retargets a click.
    TimelineTool(u64),
    /// One visible Timeline thought summary, keyed by a monotonic interned id.
    TimelineThought(u64),
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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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
    pub focus_manager: FocusManager,
    pub quit: bool,
    pub last_tick: Instant,
    /// Last pointer hover target (region + element), resolved from the hit
    /// map on `Moved` and consumed by product rendering as soft feedback.
    pub hovered: Option<(Region, Option<HitId>)>,
    /// Last known pointer position (screen coordinates), updated on every
    /// mouse event. Hover is re-derived from this after viewport changes.
    pub pointer_position: Option<(u16, u16)>,
    /// Whether a left-button press is waiting for its paired release. Some
    /// terminal transports report only release events; paired releases are
    /// suppressed because the press already activated the target.
    pub pointer_left_down: bool,

    // core input
    pub editor: Editor,
    pub binding_registry: BindingRegistry,
    pub command_catalog: Vec<command::TuiCommandEntry>,

    // session-level status
    pub status: String,
    pub queue_status: QueueStatus,
    /// Host-authoritative per-AgentInstance resource accounting.
    pub agent_usage: Vec<piko_protocol::AgentUsageSummary>,
    pub usage_scroll: usize,
    pub spinner_frame: usize,

    // panels (each owns its own state + render)
    /// All per-agent projections plus session-wide durable entries.
    pub timelines: TimelineStore,
    pub approvals: ApprovalPanel,
    pub mcp: crate::features::mcp::McpPanel,
    pub processes: crate::features::processes::ProcessPanel,
    pub diagnostics: crate::features::diagnostics::DiagnosticsPanel,
    pub history: HistoryPanel,
    /// Last known root input id for `/diff` when no work is actively running.
    pub last_root_input_id: Option<String>,
    /// Last push/result work diff for offline re-open via `/diff`.
    pub last_agent_work_diff: Option<piko_protocol::AgentWorkDiffEvent>,
    pub interactions: ToolInteractionPanel,
    pub sessions: SessionList,
    pub models: ModelSelector,
    pub thinking: ThinkingSelector,
    pub thought_inspector: Option<ThoughtInspectorState>,
    /// Model chosen in stage one of the model → thinking workflow.
    pub pending_model: Option<ModelOption>,
    pub settings: SettingsPanel,
    pub tree: TreePanel,
    pub summary_prompt: Option<ChoiceWorkflow>,
    pub auth_selector: AuthSelector,
    /// True while the Tree surface is choosing a branch point for `/fork`.
    pub tree_fork_mode: bool,

    // agent panel (multi-agent switching)
    pub agent_panel: AgentPanelState,

    // notifications
    pub notifications: NotificationCenter,

    /// Host-projected todo lists by agent instance id (F-27 `/todo` overlay).
    pub todo_lists: TodoListsState,

    // tui config (from hostd settings under `tui` namespace)
    pub tui_config: TuiConfig,

    // host runtime settings mirror (ConfigGet namespace "host")
    pub host_settings: HostRuntimeSettings,

    // active theme (resolved color tokens)
    pub theme: Theme,
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
    pub pending_submit_content: Option<piko_protocol::MessageContent>,
    pending_submit_draft: Option<crate::features::editor::state::EditorDraft>,
    pub requested_id: Option<String>,
    pub continue_requested: bool,
    /// Host-authoritative AgentInput work projection.
    pub agent_work: HashMap<String, piko_protocol::AgentWorkSnapshot>,
    pending_submissions: HashMap<String, pending::PendingSubmissionUi>,
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

pub use helpers::{command_id, get_active_branch_entries};
pub(crate) use helpers::{config_command_for_setting, flatten_models};
