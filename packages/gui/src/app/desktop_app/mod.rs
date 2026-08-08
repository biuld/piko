//! Root DesktopApp view: owns ClientBridge, island Entities, and chrome.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::*;
use gpui_component::input::{InputEvent, InputState};

use crate::bridge::ClientBridge;
use crate::features::{
    AgentsIsland, ComposerIsland, SETTINGS_FOCUS_ORDER, SessionsIsland, SettingsIslandId,
    SettingsNavIsland, SettingsPanelIsland, TimelineIsland, TreeIsland,
};
use crate::features::{AppNotificationCenter, CommandPalette, InteractionForm, SettingsSection};
use crate::shell::{
    ALL_ISLAND_IDS, FocusCycleDir, IslandFocusRing, IslandFocusTable, IslandId, OverlayHost,
    SettingsFrameChrome, mount_settings_frame, mount_workbench_frame,
};
use crate::theme::tokens;
use gpui_component::Root;
use piko_client_core::{ClientIntent, ClientState};
use piko_protocol::SessionListScope;

use super::archipelago::{AppArchipelago, ArchipelagoFocusTarget, ArchipelagoId};
use super::layout_state::LayoutState;
use super::submit_recovery::{FirstSubmitRecovery, SubmitRecovery};
use super::timeline_follow::TimelineContentFp;
use super::ux_prefs::GuiUxPrefs;
use super::wiring::archipelago_nav::SettingsFocusRing;
use crate::config::HostRuntimeSettings;

actions!(
    piko,
    [
        FocusComposer,
        FocusNextIsland,
        FocusPrevIsland,
        NewSession,
        CancelTurn,
        JumpToLatest,
        ToggleSessions,
        ToggleRightColumn,
        Quit,
        OpenCommandPalette,
        CloseTransientOverlay,
        OpenSettings,
        ToggleNotificationCenter,
    ]
);

const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub struct DesktopApp {
    pub(crate) bridge: ClientBridge,
    pub(crate) cwd: String,
    focus_handle: FocusHandle,
    pub(crate) sessions: Entity<SessionsIsland>,
    pub(crate) timeline: Entity<TimelineIsland>,
    pub(crate) composer: Entity<ComposerIsland>,
    pub(crate) agents: Entity<AgentsIsland>,
    pub(crate) tree: Entity<TreeIsland>,
    pub(crate) composer_input: Entity<InputState>,
    pub(crate) drafts: HashMap<String, String>,
    pub(crate) no_session_draft: String,
    pub(crate) follow_bottom: HashMap<String, bool>,
    pub(crate) timeline_offsets: HashMap<String, Point<Pixels>>,
    pub(crate) last_selected_agent: Option<String>,
    pub(crate) last_timeline_fp: TimelineContentFp,
    pub(crate) pending_scroll_bottom: bool,
    pub(crate) submit_recovery: SubmitRecovery,
    pub(crate) pending_first_submit: FirstSubmitRecovery,
    pub(crate) clear_composer_on_render: bool,
    pub(crate) overlay: OverlayHost,
    pub(crate) overlay_focus_restore: Option<ArchipelagoFocusTarget>,
    pub(crate) interaction_form: Option<Entity<InteractionForm>>,
    pub(crate) command_palette: Option<Entity<CommandPalette>>,
    pub(crate) layout: LayoutState,
    pub(crate) tree_preview_entry_id: Option<String>,
    pub(crate) tree_expanded_by_agent: super::island_actions::TreeExpandedByAgent,
    pub(crate) pending_timeline_scroll_id: Option<String>,
    pub(crate) ux_prefs: GuiUxPrefs,
    pub(crate) last_notified_error: Option<String>,
    pub(crate) last_connection_connected: bool,
    pub(crate) notifications: AppNotificationCenter,
    pub(crate) last_live_session_for_draft: Option<String>,
    gui_config_fingerprint: Option<String>,
    pub(crate) host_config_fingerprint: Option<String>,
    pub(crate) host_runtime: HostRuntimeSettings,
    pub(crate) island_focus: IslandFocusRing,
    /// Chrome focus registry ([`IslandView`] slots). Prefer this over matching
    /// on concrete island Entities for ring / keyboard handoff.
    pub(crate) island_focus_table: IslandFocusTable<IslandId>,
    /// Settings archipelago body islands (Nav | Panel).
    pub(crate) settings_nav: Entity<SettingsNavIsland>,
    pub(crate) settings_panel: Entity<SettingsPanelIsland>,
    pub(crate) settings_focus: SettingsFocusRing,
    pub(crate) settings_focus_table: IslandFocusTable<SettingsIslandId>,
    pub(crate) fp_sessions: Option<String>,
    pub(crate) fp_timeline: Option<u64>,
    pub(crate) fp_composer: Option<String>,
    pub(crate) fp_agents: Option<String>,
    pub(crate) fp_tree: Option<String>,
    pub(crate) last_chrome_fp: Option<String>,
    pub(crate) archipelago: AppArchipelago,
    pub(crate) last_settings_section: SettingsSection,
    pub(crate) pinned_session_ids: HashSet<String>,
    pub(crate) session_last_used_at_ms: HashMap<String, u64>,
    pub(crate) session_rename_input: Option<Entity<InputState>>,
}

mod app_impl;
mod traits;
