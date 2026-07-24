//! Composer, Agent selection, draft recovery, and Timeline follow behavior.

use std::collections::HashSet;

use gpui::*;
use piko_client_core::state::PendingOp;
use piko_client_core::{ClientIntent, SessionPhase};

use crate::features::{NotificationSeverity, derive_composer};
use crate::projections::normalize_cwd_key;
use crate::shell::IslandId;

use super::desktop_app::{CancelTurn, DesktopApp, FocusComposer, JumpToLatest, NewSession};
use super::timeline_follow::{TimelineContentFp, is_near_bottom, should_scroll_on_growth};

impl DesktopApp {
    pub(crate) fn follow_for_selected(&self) -> bool {
        self.selected_agent_id()
            .and_then(|id| self.follow_bottom.get(&id).copied())
            .unwrap_or(false)
    }

    /// `cmd-n`: New Session in the live Session's cwd, or Open Directory when idle.
    pub(crate) fn action_new_session(
        &mut self,
        _: &NewSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.trigger_new_session(window, cx);
    }

    /// Shared `session.new` behavior for the `cmd-n` keybinding and the host
    /// catalog's `session.new` id run from the Command Palette.
    pub(crate) fn trigger_new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(cwd) = self
            .bridge_state()
            .live_session
            .as_ref()
            .map(|s| s.cwd.clone())
        {
            self.create_session_in_cwd(cwd, cx);
        } else {
            self.action_open_directory(window, cx);
        }
    }

    pub(crate) fn action_open_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(crate::t!(
                "island.sessions.action.open_directory"
            ))),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = rx.await else {
                return;
            };
            let paths = match result {
                Ok(Some(paths)) => paths,
                _ => return,
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let cwd = path.to_string_lossy().into_owned();
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.handle_directory_picked(cwd, window, cx);
                });
            });
        })
        .detach();
    }

    pub(crate) fn create_session_in_cwd(&mut self, cwd: String, cx: &mut Context<Self>) {
        self.persist_composer_draft(cx);
        self.bridge.intent(ClientIntent::CreateSession { cwd });
        self.refresh_islands(cx);
        cx.notify();
    }

    fn handle_directory_picked(
        &mut self,
        cwd: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session_list_has_cwd(&cwd) {
            let path = cwd.clone();
            self.push_app_notification(
                NotificationSeverity::Info,
                crate::t!("island.sessions.notice.directory_exists.title"),
                crate::t!(
                    "island.sessions.notice.directory_exists.message",
                    path = path
                ),
                window,
                cx,
            );
            return;
        }
        self.create_session_in_cwd(cwd, cx);
    }

    fn session_list_has_cwd(&self, cwd: &str) -> bool {
        let key = normalize_cwd_key(cwd);
        self.bridge_state()
            .session_list
            .sessions
            .iter()
            .any(|s| normalize_cwd_key(&s.cwd) == key)
    }

    pub(crate) fn action_cancel_turn(
        &mut self,
        _: &CancelTurn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.bridge.intent(ClientIntent::CancelTurn);
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn action_focus_composer(
        &mut self,
        _: &FocusComposer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_island(IslandId::Composer, window, cx);
    }

    pub(crate) fn action_jump_to_latest(
        &mut self,
        _: &JumpToLatest,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(agent) = self.selected_agent_id() {
            self.follow_bottom.insert(agent, true);
            self.pending_scroll_bottom = true;
            self.timeline.update(cx, |island, cx| {
                island.scroll_to_bottom();
                island.set_follow(true, cx);
            });
            cx.notify();
        }
    }

    pub(crate) fn handle_open_session(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.persist_composer_draft(cx);
        self.follow_bottom.clear();
        self.timeline_offsets.clear();
        self.last_selected_agent = None;
        self.last_timeline_fp = TimelineContentFp::default();
        self.pending_scroll_bottom = false;
        self.timeline.update(cx, |island, _cx| {
            island.set_offset(point(px(0.), px(0.)));
        });
        self.bridge.intent(ClientIntent::OpenSession {
            session_id: session_id.clone(),
            session_path: None,
        });
        self.bump_session_mru(&session_id, cx);
        cx.notify();
    }

    pub(crate) fn handle_select_agent(
        &mut self,
        agent_instance_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.persist_composer_draft(cx);
        if let Some(current) = self.selected_agent_id() {
            let offset = self.timeline.read(cx).offset();
            self.timeline_offsets.insert(current, offset);
        }
        self.bridge.intent(ClientIntent::SelectAgent {
            agent_instance_id: agent_instance_id.clone(),
        });
        self.load_draft_into_composer(window, cx);
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn sync_selected_agent_scroll(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_agent_id();
        if selected == self.last_selected_agent {
            return;
        }
        if let Some(previous) = self.last_selected_agent.take() {
            let offset = self.timeline.read(cx).offset();
            self.timeline_offsets.entry(previous).or_insert(offset);
        }
        if let Some(agent) = selected.clone() {
            match self.follow_bottom.get(&agent).copied() {
                Some(true) => self.pending_scroll_bottom = true,
                Some(false) => {
                    if let Some(offset) = self.timeline_offsets.get(&agent).copied() {
                        self.timeline.update(cx, |island, _cx| {
                            island.set_offset(offset);
                        });
                    }
                }
                None => {
                    self.follow_bottom.insert(agent, false);
                    self.pending_scroll_bottom = false;
                    self.timeline.update(cx, |island, _cx| {
                        island.set_offset(point(px(0.), px(0.)));
                    });
                }
            }
        }
        self.last_selected_agent = selected;
    }

    pub(crate) fn submit_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer_input.read(cx).value().to_string();
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        if self.bridge.state().session_phase == SessionPhase::IdleNoSession {
            let before: HashSet<String> = self
                .bridge
                .state()
                .pending_commands
                .keys()
                .cloned()
                .collect();
            self.bridge.intent(ClientIntent::CreateSession {
                cwd: self.cwd.clone(),
            });
            if let Some(command_id) =
                self.bridge
                    .state()
                    .pending_commands
                    .iter()
                    .find_map(|(id, op)| {
                        (!before.contains(id) && matches!(op, PendingOp::Create))
                            .then(|| id.clone())
                    })
            {
                self.pending_first_submit.track(command_id, text);
            }
            self.refresh_islands(cx);
            cx.notify();
            return;
        }

        let composer = derive_composer(self.bridge.state());
        if !composer.can_send {
            return;
        }

        self.send_tracked_submit(trimmed, text.clone());
        self.composer_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        if let Some(sid) = self.live_session_id() {
            self.drafts.insert(sid, String::new());
        } else {
            self.no_session_draft.clear();
        }
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn send_tracked_submit(&mut self, text: String, recovery_text: String) {
        let before: HashSet<String> = self
            .bridge
            .state()
            .pending_commands
            .keys()
            .cloned()
            .collect();
        self.bridge.intent(ClientIntent::SubmitTurn { text });
        if let Some(command_id) =
            self.bridge
                .state()
                .pending_commands
                .iter()
                .find_map(|(id, op)| {
                    (!before.contains(id) && matches!(op, PendingOp::Submit)).then(|| id.clone())
                })
        {
            self.submit_recovery.track(command_id, recovery_text);
        }
        if let Some(agent) = self.selected_agent_id() {
            self.follow_bottom.insert(agent, true);
            self.pending_scroll_bottom = true;
        }
    }

    pub(crate) fn on_bridge_polled(&mut self, _before_err: Option<String>) {
        self.submit_recovery.observe(self.bridge.state());

        if let Some(text) = self.pending_first_submit.resolve(self.bridge.state()) {
            self.send_tracked_submit(text.trim().to_string(), text);
            self.clear_composer_on_render = true;
        }
    }

    pub(crate) fn sync_timeline_follow(&mut self, cx: &mut Context<Self>) {
        let timeline = crate::features::derive_timeline(self.bridge_state());
        let fp = TimelineContentFp {
            agent_id: self.selected_agent_id(),
            row_count: timeline.rows.len(),
            body_chars: timeline.rows.iter().map(|r| r.body.len()).sum(),
        };
        let content_grew = fp != self.last_timeline_fp;
        self.last_timeline_fp = fp;

        let follow = self.follow_for_selected();
        if should_scroll_on_growth(follow, content_grew, self.pending_scroll_bottom) {
            self.timeline.update(cx, |island, cx| {
                island.scroll_to_bottom();
                island.set_follow(true, cx);
            });
            self.pending_scroll_bottom = false;
            return;
        }

        if content_grew {
            return;
        }

        let near = is_near_bottom(self.timeline.read(cx).scroll_handle());
        if let Some(agent) = self.selected_agent_id()
            && follow
            && !near
        {
            self.follow_bottom.insert(agent, false);
            self.timeline.update(cx, |island, cx| {
                island.set_follow(false, cx);
            });
        }
    }

    pub(crate) fn maybe_load_draft_on_live(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live_id = self.live_session_id();
        if live_id != self.last_live_session_for_draft {
            self.last_live_session_for_draft = live_id;
            if self.last_live_session_for_draft.is_some() {
                self.load_draft_into_composer(window, cx);
            }
        }
    }

    pub(crate) fn apply_pending_composer_restore(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.clear_composer_on_render {
            self.clear_composer_on_render = false;
            self.composer_input.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        }
        let restore = self.submit_recovery.take_restore();
        if !restore.is_empty() {
            let current = self.composer_input.read(cx).value().to_string();
            if current.trim().is_empty() {
                let text = restore.join("\n");
                self.composer_input.update(cx, |state, cx| {
                    state.set_value(text, window, cx);
                });
            } else {
                self.submit_recovery.defer_restore(restore);
            }
        }
    }

    pub(crate) fn persist_composer_draft(&mut self, cx: &Context<Self>) {
        let text = self.composer_input.read(cx).value().to_string();
        if let Some(sid) = self.live_session_id() {
            self.drafts.insert(sid, text);
        } else {
            self.no_session_draft = text;
        }
    }

    pub(crate) fn load_draft_into_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .live_session_id()
            .and_then(|sid| self.drafts.get(&sid).cloned())
            .unwrap_or_else(|| self.no_session_draft.clone());
        self.composer_input.update(cx, |state, cx| {
            state.set_value(text, window, cx);
        });
    }

    pub(crate) fn live_session_id(&self) -> Option<String> {
        self.bridge
            .state()
            .live_session
            .as_ref()
            .map(|s| s.session_id.clone())
    }

    pub(crate) fn selected_agent_id(&self) -> Option<String> {
        self.bridge
            .state()
            .live_session
            .as_ref()
            .and_then(|s| s.selected_agent.clone())
    }
}
