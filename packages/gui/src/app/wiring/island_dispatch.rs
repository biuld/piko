//! Routes `IslandMsg` from Entity islands to DesktopApp handlers.

use gpui::*;

use crate::shell::{IslandId, IslandMsg};

use crate::app::desktop_app::{CancelTurn, DesktopApp, JumpToLatest};

/// Deliver an island message after the current effect cycle.
///
/// Island click/focus handlers run inside that island's `Entity::update`. Host
/// handlers often update the same island again (focus chrome, dirty push),
/// which panics if done synchronously ("cannot update while already being
/// updated"). Deferring drops the island off the GPUI update stack first.
pub(crate) fn schedule_island_msg(
    host: WeakEntity<DesktopApp>,
    id: IslandId,
    msg: IslandMsg,
    window: &Window,
    cx: &mut App,
) {
    window.defer(cx, move |window, cx| {
        if let Some(host) = host.upgrade() {
            host.update(cx, |app, cx| {
                app.dispatch_island_msg(id, msg, window, cx);
            });
        }
    });
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
        match msg {
            IslandMsg::ClaimFocus => {
                self.focus_island(id, window, cx);
                return;
            }
            IslandMsg::FocusIsland { id: target } => {
                self.focus_island(target, window, cx);
                return;
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
        }
        self.refresh_islands(cx);
        cx.notify();
    }
}
