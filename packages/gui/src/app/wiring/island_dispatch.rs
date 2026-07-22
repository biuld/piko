//! Routes `IslandMsg` from Entity islands to DesktopApp handlers.
//!
//! Focus layer: [`IslandMessage`] + [`route_focus_message`] (chrome).
//! Defer + host sink: [`schedule_island_message`] / [`IslandHost`].

use gpui::*;

use crate::shell::{
    IslandHost, IslandId, IslandMessage, IslandMsg, route_focus_message, schedule_island_message,
};

use crate::app::desktop_app::{CancelTurn, DesktopApp, JumpToLatest};

/// App-facing alias: deferred island → [`DesktopApp`] message delivery.
pub(crate) fn schedule_island_msg(
    host: WeakEntity<DesktopApp>,
    id: IslandId,
    msg: IslandMsg,
    window: &Window,
    cx: &mut App,
) {
    schedule_island_message(host, id, msg, window, cx);
}

impl IslandHost for DesktopApp {
    type Id = IslandId;
    type Msg = IslandMsg;

    fn handle_island_message(
        &mut self,
        from: IslandId,
        msg: IslandMsg,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_island_msg(from, msg, window, cx);
    }
}

impl DesktopApp {
    /// Resolve a message emitted by an island. `id` identifies the emitting
    /// island (used for focus-claim bookkeeping).
    pub(crate) fn dispatch_island_msg(
        &mut self,
        id: IslandId,
        msg: IslandMsg,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Chrome focus layer — must not fall through to product refresh.
        if let Some(focus) = msg.as_focus_msg() {
            let _ = route_focus_message(
                &self.island_focus_table,
                &mut *self.island_focus,
                id,
                focus,
                window,
                cx,
            );
            return;
        }

        match msg {
            // Focus variants handled above via IslandMessage.
            IslandMsg::ClaimFocus | IslandMsg::FocusIsland { .. } => {
                unreachable!("focus IslandMsg must be classified by IslandMessage::as_focus_msg")
            }
            IslandMsg::OpenSession { session_id } => self.handle_open_session(session_id, cx),
            IslandMsg::NewSession { cwd } => self.create_session_in_cwd(cwd, cx),
            IslandMsg::OpenDirectory => self.action_open_directory(window, cx),
            IslandMsg::SelectAgent { agent_instance_id } => {
                self.handle_select_agent(agent_instance_id, window, cx)
            }
            IslandMsg::TreeActivate { entry_id } => self.handle_tree_activate(entry_id, window, cx),
            IslandMsg::TreeToggleExpand { entry_id } => {
                self.handle_tree_toggle_expand(entry_id, cx)
            }
            IslandMsg::JumpToLatest => self.action_jump_to_latest(&JumpToLatest, window, cx),
            // Local UI state already updated inside TimelineIsland / ComposerIsland.
            IslandMsg::ToggleToolDetail { .. } | IslandMsg::ToggleActivity => {}
            IslandMsg::ActivityActivate {
                agent_instance_id,
                prompt_id,
            } => self.handle_activity_item(agent_instance_id, prompt_id, window, cx),
            IslandMsg::SubmitComposer => self.submit_composer(window, cx),
            IslandMsg::CancelTurn => self.action_cancel_turn(&CancelTurn, window, cx),
            IslandMsg::RenameSession { session_id } => {
                self.handle_rename_session_request(session_id, window, cx);
            }
            IslandMsg::DeleteSession { session_id } => {
                self.handle_delete_session_request(session_id, cx);
            }
            IslandMsg::TogglePinSession { session_id } => {
                self.handle_toggle_pin_session(&session_id, cx);
            }
        }
        self.refresh_islands(cx);
        cx.notify();
    }
}
