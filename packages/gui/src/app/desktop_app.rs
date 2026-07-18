//! Root DesktopApp view: owns ClientBridge, polling, and top-level layout.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::*;
use gpui_component::input::{InputEvent, InputState};

use crate::bridge::ClientBridge;
use crate::shell::derive_status_bar;
use piko_client_core::{ClientIntent, ClientState};
use piko_protocol::SessionListScope;

use super::layout_state::LayoutState;
use super::status_bar::render_status_bar;
use super::submit_recovery::{FirstSubmitRecovery, SubmitRecovery};
use super::timeline_follow::TimelineContentFp;
use super::title_bar::render_title_bar;
use super::ux_prefs::GuiUxPrefs;
use crate::theme::metrics;
use crate::theme::tokens;
use gpui_component::Root;

actions!(
    piko,
    [
        FocusComposer,
        NewSession,
        CancelTurn,
        JumpToLatest,
        ToggleSessions,
        ToggleInspector
    ]
);

const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub struct DesktopApp {
    pub(super) bridge: ClientBridge,
    pub(super) cwd: String,
    focus_handle: FocusHandle,
    pub(super) composer_input: Entity<InputState>,
    pub(super) drafts: HashMap<String, String>,
    pub(super) no_session_draft: String,
    pub(super) follow_bottom: HashMap<String, bool>,
    pub(super) timeline_offsets: HashMap<String, Point<Pixels>>,
    pub(super) last_selected_agent: Option<String>,
    pub(super) timeline_scroll: ScrollHandle,
    pub(super) last_timeline_fp: TimelineContentFp,
    pub(super) pending_scroll_bottom: bool,
    pub(super) submit_recovery: SubmitRecovery,
    pub(super) pending_first_submit: FirstSubmitRecovery,
    pub(super) clear_composer_on_render: bool,
    pub(super) expanded_tools: HashSet<String>,
    pub(super) activity_expanded: bool,
    pub(super) activity_user_toggled: bool,
    pub(super) activity_actionable_fp: String,
    pub(super) open_prompt_fp: Option<String>,
    pub(super) open_prompt_flight: Option<bool>,
    pub(super) layout: LayoutState,
    pub(super) map_preview_entry_id: Option<String>,
    pub(super) map_expanded_by_agent: super::workbench_chrome::MapExpandedByAgent,
    pub(super) pending_timeline_scroll_id: Option<String>,
    pub(super) ux_prefs: GuiUxPrefs,
    pub(super) last_notified_error: Option<String>,
    pub(super) last_connection_connected: bool,
    pub(super) last_live_session_for_draft: Option<String>,
    gui_config_fingerprint: Option<String>,
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
                .auto_grow(1, 8)
                .placeholder("Message… (Enter to send, Shift+Enter for newline)")
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
                                this.on_bridge_polled(before_err);
                                this.sync_timeline_follow();
                                this.sync_activity_expand();
                                if this.bridge.state().is_live() {
                                    this.prune_map_state_after_reconcile();
                                }
                                cx.notify();
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
            composer_input,
            drafts: HashMap::new(),
            no_session_draft: String::new(),
            follow_bottom: HashMap::new(),
            timeline_offsets: HashMap::new(),
            last_selected_agent: None,
            timeline_scroll: ScrollHandle::new(),
            last_timeline_fp: TimelineContentFp::default(),
            pending_scroll_bottom: false,
            submit_recovery: SubmitRecovery::default(),
            pending_first_submit: FirstSubmitRecovery::default(),
            clear_composer_on_render: false,
            expanded_tools: HashSet::new(),
            activity_expanded: false,
            activity_user_toggled: false,
            activity_actionable_fp: String::new(),
            open_prompt_fp: None,
            open_prompt_flight: None,
            layout: LayoutState::default(),
            map_preview_entry_id: None,
            map_expanded_by_agent: std::collections::HashMap::new(),
            pending_timeline_scroll_id: None,
            ux_prefs: GuiUxPrefs::default(),
            last_notified_error: None,
            last_connection_connected: true,
            last_live_session_for_draft: None,
            gui_config_fingerprint: None,
        }
    }

    pub fn bootstrap(&mut self) {
        self.bridge.intent(ClientIntent::DiscoverSessions {
            scope: SessionListScope::CurrentFolder,
            cwd: Some(self.cwd.clone()),
        });
        self.bridge.intent(ClientIntent::ListModels);
        self.bridge.request_gui_config();
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
                self.layout.inspector_width = settings.inspector_width;
                self.layout.prefer_session_open = settings.session_open;
                self.layout.prefer_inspector_open = settings.inspector_open;
                self.ux_prefs.prefer_reduced_motion = settings.reduced_motion;
                self.gui_config_fingerprint = Some(fingerprint);
            }
            Err(error) => {
                log::warn!("invalid [gui] settings ignored: {error}");
            }
        }
    }

    pub(crate) fn persist_gui_config(&mut self) {
        let settings = crate::config::GuiSettings {
            session_width: self.layout.session_width,
            inspector_width: self.layout.inspector_width,
            session_open: self.layout.prefer_session_open,
            inspector_open: self.layout.prefer_inspector_open,
            reduced_motion: self.ux_prefs.prefer_reduced_motion,
        };
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
        self.sync_selected_agent_scroll();
        self.sync_timeline_follow();
        self.apply_pending_timeline_scroll();
        self.sync_activity_expand();
        self.sync_prompts(window, cx);
        self.maybe_close_session_sheet_on_live(window, cx);
        self.sync_notifications(window, cx);
        self.maybe_load_draft_on_live(window, cx);

        let on_new = cx.listener(Self::action_new_session);
        let on_cancel = cx.listener(Self::action_cancel_turn);
        let on_focus = cx.listener(Self::action_focus_composer);
        let on_jump = cx.listener(Self::action_jump_to_latest);
        let on_sessions = cx.listener(Self::action_toggle_sessions);
        let on_inspector = cx.listener(Self::action_toggle_inspector);
        let show_session = self.layout.show_session_pane();
        let status_vm = derive_status_bar(self.bridge.state(), show_session);
        let t = tokens();
        let m = metrics();
        let allow_motion = self.ux_prefs.allow_motion();
        let project_name = std::path::Path::new(&self.cwd)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "workspace".into());

        div()
            .id("desktop-app")
            .track_focus(&self.focus_handle)
            .on_action(on_new)
            .on_action(on_cancel)
            .on_action(on_focus)
            .on_action(on_jump)
            .on_action(on_sessions)
            .on_action(on_inspector)
            .key_context("DesktopApp")
            .size_full()
            .flex()
            .flex_col()
            .bg(t.canvas_rgba())
            .text_color(t.fg_rgba())
            .child(render_title_bar(self.bridge.state(), &project_name))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .p(m.island_gutter)
                    .child(self.render_workbench_row(window, cx)),
            )
            .child(render_status_bar(&status_vm, allow_motion))
            .children(Root::render_dialog_layer(window, cx))
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
