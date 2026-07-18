//! Composer, Agent selection, draft recovery, and Timeline follow behavior.

use std::collections::HashSet;

use gpui::*;
use gpui_component::input::InputState;
use piko_client_core::state::PendingOp;
use piko_client_core::{ClientIntent, SessionPhase};

use crate::workbench::{derive_composer, derive_timeline};

use super::desktop_app::{CancelTurn, DesktopApp, FocusComposer, JumpToLatest, NewSession};
use super::model_cycle::{next_model_intent, next_thinking_intent};
use super::timeline_follow::{TimelineContentFp, is_near_bottom, should_scroll_on_growth};

impl DesktopApp {
    pub(crate) fn composer_input_entity(&self) -> &Entity<InputState> {
        &self.composer_input
    }

    pub(crate) fn timeline_scroll_handle(&self) -> &ScrollHandle {
        &self.timeline_scroll
    }

    pub(crate) fn follow_for_selected(&self) -> bool {
        self.selected_agent_id()
            .and_then(|id| self.follow_bottom.get(&id).copied())
            .unwrap_or(true)
    }

    pub(crate) fn action_new_session(
        &mut self,
        _: &NewSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.persist_composer_draft(cx);
        self.bridge.intent(ClientIntent::CreateSession {
            cwd: self.cwd.clone(),
        });
        cx.notify();
    }

    pub(crate) fn action_cancel_turn(
        &mut self,
        _: &CancelTurn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.bridge.intent(ClientIntent::CancelTurn);
        cx.notify();
    }

    pub(crate) fn action_focus_composer(
        &mut self,
        _: &FocusComposer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer_input.update(cx, |state, cx| {
            state.focus(window, cx);
        });
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
            self.timeline_scroll.scroll_to_bottom();
            cx.notify();
        }
    }

    pub(crate) fn handle_open_session(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.persist_composer_draft(cx);
        self.bridge.intent(ClientIntent::OpenSession {
            session_id,
            session_path: None,
        });
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
            self.timeline_offsets
                .insert(current, self.timeline_scroll.offset());
        }
        self.bridge.intent(ClientIntent::SelectAgent {
            agent_instance_id: agent_instance_id.clone(),
        });
        self.load_draft_into_composer(window, cx);
        cx.notify();
    }

    pub(crate) fn sync_selected_agent_scroll(&mut self) {
        let selected = self.selected_agent_id();
        if selected == self.last_selected_agent {
            return;
        }
        if let Some(previous) = self.last_selected_agent.take() {
            self.timeline_offsets
                .entry(previous)
                .or_insert_with(|| self.timeline_scroll.offset());
        }
        if let Some(agent) = selected.clone() {
            if self.follow_bottom.get(&agent).copied().unwrap_or(true) {
                self.pending_scroll_bottom = true;
            } else if let Some(offset) = self.timeline_offsets.get(&agent).copied() {
                self.timeline_scroll.set_offset(offset);
            }
        }
        self.last_selected_agent = selected;
    }

    pub(crate) fn cycle_model(&mut self, cx: &mut Context<Self>) {
        let model = &self.bridge.state().model;
        if let Some(intent) = next_model_intent(
            &model.providers,
            model.provider.as_deref(),
            model.model_id.as_deref(),
        ) {
            self.bridge.intent(intent);
            cx.notify();
        }
    }

    pub(crate) fn cycle_thinking(&mut self, cx: &mut Context<Self>) {
        let current = self.bridge.state().model.thinking_level.clone();
        self.bridge.intent(next_thinking_intent(current.as_deref()));
        cx.notify();
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
        if let Some(agent) = self.selected_agent_id() {
            self.follow_bottom.insert(agent, true);
            self.pending_scroll_bottom = true;
        }
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
    }

    pub(crate) fn on_bridge_polled(&mut self, _before_err: Option<String>) {
        self.submit_recovery.observe(self.bridge.state());

        if let Some(text) = self.pending_first_submit.resolve(self.bridge.state()) {
            self.send_tracked_submit(text.trim().to_string(), text);
            self.clear_composer_on_render = true;
        }
    }

    pub(crate) fn sync_timeline_follow(&mut self) {
        let timeline = derive_timeline(self.bridge.state());
        let fp = TimelineContentFp {
            agent_id: self.selected_agent_id(),
            row_count: timeline.rows.len(),
            body_chars: timeline.rows.iter().map(|r| r.body.len()).sum(),
        };
        let content_grew = fp != self.last_timeline_fp;
        self.last_timeline_fp = fp;

        let follow = self.follow_for_selected();
        if should_scroll_on_growth(follow, content_grew, self.pending_scroll_bottom) {
            self.timeline_scroll.scroll_to_bottom();
            self.pending_scroll_bottom = false;
            return;
        }

        let near = is_near_bottom(&self.timeline_scroll);
        if let Some(agent) = self.selected_agent_id() {
            if follow && !near {
                self.follow_bottom.insert(agent, false);
            } else if !follow && near {
                self.follow_bottom.insert(agent, true);
            }
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
