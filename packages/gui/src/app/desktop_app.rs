//! Root DesktopApp view: owns ClientBridge, island Entities, and chrome.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::*;
use gpui_component::input::{InputEvent, InputState};

use crate::bridge::ClientBridge;
use crate::features::{AgentsIsland, ComposerIsland, SessionsIsland, TimelineIsland, TreeIsland};
use crate::features::{
    CommandPalette, InteractionForm, SettingsSection,
    settings::{render_nav, render_panel},
};
use crate::shell::{
    FocusCycleDir, IslandFocusRing, OverlayHost, mount_settings_frame, mount_workbench_frame,
};
use crate::theme::tokens;
use gpui_component::Root;
use piko_client_core::{ClientIntent, ClientState};
use piko_protocol::SessionListScope;

use super::layout_state::LayoutState;
use super::primary_surface::PrimarySurface;
use super::submit_recovery::{FirstSubmitRecovery, SubmitRecovery};
use super::timeline_follow::TimelineContentFp;
use super::ux_prefs::GuiUxPrefs;
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
    pub(crate) interaction_form: Option<Entity<InteractionForm>>,
    pub(crate) command_palette: Option<Entity<CommandPalette>>,
    pub(crate) layout: LayoutState,
    pub(crate) tree_preview_entry_id: Option<String>,
    pub(crate) tree_expanded_by_agent: super::island_actions::TreeExpandedByAgent,
    pub(crate) pending_timeline_scroll_id: Option<String>,
    pub(crate) ux_prefs: GuiUxPrefs,
    pub(crate) last_notified_error: Option<String>,
    pub(crate) last_connection_connected: bool,
    pub(crate) last_live_session_for_draft: Option<String>,
    gui_config_fingerprint: Option<String>,
    pub(crate) host_config_fingerprint: Option<String>,
    pub(crate) host_runtime: HostRuntimeSettings,
    pub(crate) island_focus: IslandFocusRing,
    pub(crate) fp_sessions: Option<String>,
    pub(crate) fp_timeline: Option<String>,
    pub(crate) fp_composer: Option<String>,
    pub(crate) fp_agents: Option<String>,
    pub(crate) fp_tree: Option<String>,
    pub(crate) last_chrome_fp: Option<String>,
    pub(crate) primary_surface: PrimarySurface,
    pub(crate) last_settings_section: SettingsSection,
    pub(crate) pinned_session_ids: HashSet<String>,
    pub(crate) session_last_used_at_ms: HashMap<String, u64>,
    pub(crate) session_rename_input: Option<Entity<InputState>>,
}

impl DesktopApp {
    pub fn new(
        bridge: ClientBridge,
        cwd: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer_input = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(3, 12)
                .placeholder(crate::t!("composer.placeholder"))
        });

        cx.subscribe_in(
            &composer_input,
            window,
            |this, _state, event, window, cx| {
                if let InputEvent::PressEnter { secondary } = event
                    && !secondary
                {
                    this.submit_composer(window, cx);
                }
            },
        )
        .detach();

        let host = cx.entity().downgrade();
        let sessions = cx.new(|cx| SessionsIsland::new(host.clone(), window, cx));
        sessions.update(cx, |island, cx| {
            island.subscribe_search(window, cx);
        });
        let timeline = cx.new(|cx| TimelineIsland::new(host.clone(), cx));
        let composer = cx.new(|cx| ComposerIsland::new(host.clone(), composer_input.clone(), cx));
        let agents = cx.new(|cx| AgentsIsland::new(host.clone(), cx));
        let tree = cx.new(|cx| TreeIsland::new(host, cx));

        let entity = cx.entity().downgrade();
        cx.spawn_in(window, async move |_window, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let Ok(()) = cx.update(|_, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            let before_err = this.bridge.state().last_error.clone();
                            if this.bridge.poll() {
                                this.sync_gui_config();
                                this.sync_host_runtime_config();
                                this.sync_command_catalog(cx);
                                this.on_bridge_polled(before_err);
                                this.sync_timeline_follow(cx);
                                if this.bridge.state().is_live() {
                                    this.prune_map_state_after_reconcile();
                                }
                                this.refresh_islands(cx);
                                let chrome_fp = this.chrome_fingerprint();
                                if this.last_chrome_fp.as_ref() != Some(&chrome_fp) {
                                    this.last_chrome_fp = Some(chrome_fp);
                                    cx.notify();
                                }
                            }
                        });
                    }
                }) else {
                    break;
                };
            }
        })
        .detach();

        Self {
            bridge,
            cwd,
            focus_handle: cx.focus_handle(),
            sessions,
            timeline,
            composer,
            agents,
            tree,
            composer_input,
            drafts: HashMap::new(),
            no_session_draft: String::new(),
            follow_bottom: HashMap::new(),
            timeline_offsets: HashMap::new(),
            last_selected_agent: None,
            last_timeline_fp: TimelineContentFp::default(),
            pending_scroll_bottom: false,
            submit_recovery: SubmitRecovery::default(),
            pending_first_submit: FirstSubmitRecovery::default(),
            clear_composer_on_render: false,
            overlay: OverlayHost::default(),
            interaction_form: None,
            command_palette: None,
            layout: LayoutState::default(),
            tree_preview_entry_id: None,
            tree_expanded_by_agent: HashMap::new(),
            pending_timeline_scroll_id: None,
            ux_prefs: GuiUxPrefs::default(),
            last_notified_error: None,
            last_connection_connected: true,
            last_live_session_for_draft: None,
            gui_config_fingerprint: None,
            host_config_fingerprint: None,
            host_runtime: HostRuntimeSettings::default(),
            island_focus: IslandFocusRing::default(),
            fp_sessions: None,
            fp_timeline: None,
            fp_composer: None,
            fp_agents: None,
            fp_tree: None,
            last_chrome_fp: None,
            primary_surface: PrimarySurface::Workbench,
            last_settings_section: SettingsSection::default(),
            pinned_session_ids: HashSet::new(),
            session_last_used_at_ms: HashMap::new(),
            session_rename_input: None,
        }
    }

    fn chrome_fingerprint(&self) -> String {
        format!(
            "{:?}|{:?}|{}|{}|{}|{:?}|{:?}",
            self.bridge.state().shell.connection,
            self.bridge.state().last_error,
            self.layout.sessions_open,
            self.layout.agents_open,
            self.layout.tree_open,
            self.island_focus.focused(),
            self.primary_surface,
        )
    }

    pub fn bootstrap(&mut self) {
        self.bridge.intent(ClientIntent::DiscoverSessions {
            scope: SessionListScope::All,
            cwd: None,
        });
        self.bridge.intent(ClientIntent::ListModels);
        self.bridge.intent(ClientIntent::SyncModelConfig);
        self.bridge.request_gui_config();
        self.bridge.request_host_config();
    }

    pub(crate) fn bridge_state(&self) -> &ClientState {
        self.bridge.state()
    }

    pub(crate) fn bridge_mut(&mut self) -> &mut ClientBridge {
        &mut self.bridge
    }

    fn sync_gui_config(&mut self) {
        let Some(value) = self.bridge.gui_config().cloned() else {
            return;
        };
        let fingerprint = value.to_string();
        if self.gui_config_fingerprint.as_ref() == Some(&fingerprint) {
            return;
        }
        match serde_json::from_value::<crate::config::GuiSettings>(value) {
            Ok(settings) => {
                self.layout.session_width = settings.session_width;
                self.layout.right_column_width = settings.right_column_width;
                self.layout.sessions_open = settings.session_open;
                self.layout.agents_open = settings.right_column_open;
                self.layout.tree_open = settings.right_column_open;
                self.ux_prefs.prefer_reduced_motion = settings.reduced_motion;
                self.ux_prefs.hide_thinking_block = settings.hide_thinking_block;
                self.sync_session_prefs_from_gui(&settings);
                self.gui_config_fingerprint = Some(fingerprint);
            }
            Err(error) => {
                log::warn!("invalid [gui] settings ignored: {error}");
            }
        }
    }

    pub(crate) fn persist_gui_config(&mut self) {
        let mut settings = crate::config::GuiSettings {
            session_width: self.layout.session_width,
            right_column_width: self.layout.right_column_width,
            session_open: self.layout.sessions_open,
            right_column_open: self.layout.right_column_pref_open(),
            reduced_motion: self.ux_prefs.prefer_reduced_motion,
            hide_thinking_block: self.ux_prefs.hide_thinking_block,
            pinned_session_ids: Vec::new(),
            session_last_used_at_ms: HashMap::new(),
        };
        self.session_prefs_into_gui(&mut settings);
        if let Ok(value) = serde_json::to_value(settings) {
            self.gui_config_fingerprint = Some(value.to_string());
            self.bridge.update_gui_config(value);
        }
    }
}

impl Render for DesktopApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending_composer_restore(window, cx);
        self.sync_layout_breakpoint(window);
        self.sync_selected_agent_scroll(cx);
        self.sync_timeline_follow(cx);
        self.apply_pending_timeline_scroll(cx);
        self.refresh_islands(cx);
        self.sync_prompts(window, cx);
        self.maybe_close_session_sheet_on_live(window, cx);
        self.sync_notifications(window, cx);
        self.maybe_load_draft_on_live(window, cx);

        let on_new = cx.listener(Self::action_new_session);
        let on_cancel = cx.listener(Self::action_cancel_turn);
        let on_focus = cx.listener(Self::action_focus_composer);
        let on_jump = cx.listener(Self::action_jump_to_latest);
        let on_sessions = cx.listener(Self::action_toggle_sessions);
        let on_right_column = cx.listener(Self::action_toggle_right_column);
        let on_focus_next = cx.listener(Self::action_focus_next_island);
        let on_focus_prev = cx.listener(Self::action_focus_prev_island);
        let on_palette = cx.listener(Self::action_open_command_palette);
        let on_close_overlay = cx.listener(Self::action_close_transient_overlay);
        let on_open_settings = cx.listener(Self::action_open_settings);
        let overlay = self.render_active_overlay(window, cx);

        let mut root = div()
            .id("desktop-app")
            .relative()
            .track_focus(&self.focus_handle)
            .on_action(on_new)
            .on_action(on_cancel)
            .on_action(on_focus)
            .on_action(on_jump)
            .on_action(on_sessions)
            .on_action(on_right_column)
            .on_action(on_focus_next)
            .on_action(on_focus_prev)
            .on_action(on_palette)
            .on_action(on_close_overlay)
            .on_action(on_open_settings)
            .key_context("DesktopApp")
            .size_full()
            .flex()
            .flex_col()
            .bg(tokens().canvas_rgba())
            .text_color(tokens().fg_rgba());

        root = match self.primary_surface {
            PrimarySurface::Workbench => mount_workbench_frame(root, self, window, cx),
            PrimarySurface::Settings { section } => {
                let entity = cx.entity().downgrade();
                let nav = render_nav(section, entity.clone());
                let panel = render_panel(section, self, entity.clone());
                mount_settings_frame(root, entity, nav, panel)
            }
        };

        root.children(overlay)
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

impl Focusable for DesktopApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Drop for DesktopApp {
    fn drop(&mut self) {
        self.bridge.shutdown();
    }
}

impl DesktopApp {
    pub(crate) fn action_focus_next_island(
        &mut self,
        _: &FocusNextIsland,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.primary_surface.is_workbench() {
            return;
        }
        let visible = self.visible_focus_islands();
        self.island_focus.cycle(FocusCycleDir::Next, &visible);
        let id = self.island_focus.focused();
        self.focus_island(id, window, cx);
        cx.notify();
    }

    pub(crate) fn action_focus_prev_island(
        &mut self,
        _: &FocusPrevIsland,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.primary_surface.is_workbench() {
            return;
        }
        let visible = self.visible_focus_islands();
        self.island_focus.cycle(FocusCycleDir::Prev, &visible);
        let id = self.island_focus.focused();
        self.focus_island(id, window, cx);
        cx.notify();
    }
}
