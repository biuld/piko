//! Session sidebar actions: rename/delete confirm, pin, MRU.

use gpui::*;
use gpui_component::input::InputState;
use piko_client_core::ClientIntent;

use crate::shell::IslandId;

use super::super::desktop_app::DesktopApp;

impl DesktopApp {
    pub(crate) fn session_display_name(&self, session_id: &str) -> String {
        let state = self.bridge_state();
        if let Some(summary) = state
            .session_list
            .sessions
            .iter()
            .find(|s| s.session_id == session_id)
        {
            if let Some(name) = &summary.name {
                return name.clone();
            }
            if let Some(first) = &summary.first_message {
                let truncated: String = first.chars().take(40).collect();
                if truncated.len() < first.len() {
                    return format!("{truncated}…");
                }
                return truncated;
            }
        }
        session_id[..session_id.len().min(8)].to_string()
    }

    pub(crate) fn handle_rename_session_request(
        &mut self,
        session_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial_name = self.session_display_name(&session_id);
        if !self
            .overlay
            .try_open_session_rename(session_id.clone(), initial_name.clone())
        {
            return;
        }
        self.save_overlay_focus_if_needed(IslandId::Sessions);
        if self.session_rename_input.is_none() {
            self.session_rename_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(crate::t!("island.sessions.rename.placeholder"))
            }));
        }
        if let Some(input) = &self.session_rename_input {
            input.update(cx, |state, cx| {
                state.set_value(initial_name, window, cx);
                state.focus(window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn handle_delete_session_request(
        &mut self,
        session_id: String,
        cx: &mut Context<Self>,
    ) {
        let display_name = self.session_display_name(&session_id);
        if self
            .overlay
            .try_open_session_delete_confirm(session_id, display_name)
        {
            self.save_overlay_focus_if_needed(IslandId::Sessions);
            cx.notify();
        }
    }

    pub(crate) fn confirm_session_delete(&mut self, session_id: &str, cx: &mut Context<Self>) {
        self.remove_session_from_prefs(session_id);
        self.persist_gui_config();
        self.bridge.intent(ClientIntent::DeleteSession {
            session_id: session_id.to_string(),
        });
        self.overlay.close_local_confirm();
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn confirm_session_rename(
        &mut self,
        session_id: &str,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        self.bridge.intent(ClientIntent::RenameSession {
            session_id: session_id.to_string(),
            name: trimmed.to_string(),
        });
        self.overlay.close_transient();
        self.restore_overlay_focus(window, cx);
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn handle_toggle_pin_session(&mut self, session_id: &str, cx: &mut Context<Self>) {
        self.toggle_session_pin(session_id, cx);
    }
}
