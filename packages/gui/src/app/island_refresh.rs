//! Directed dirty-push of projections into island Entities.

use gpui::*;

use crate::chrome::{IslandId, IslandSessionPhase};
use crate::islands::{
    derive_activity, derive_agent_tree, derive_composer, derive_conversation_tree, derive_timeline,
};
use crate::projections::{derive_phase_view, derive_sidebar};

use super::desktop_app::DesktopApp;

impl DesktopApp {
    /// Push projections into islands when their fingerprints change.
    /// Does not `notify` DesktopApp itself.
    pub(crate) fn refresh_islands(&mut self, cx: &mut Context<Self>) {
        let phase_view = derive_phase_view(self.bridge_state());
        let island_phase = IslandSessionPhase::from_session_phase(&phase_view);

        let sessions_vm = derive_sidebar(self.bridge_state());
        let sessions_fp = format!("{:?}", sessions_vm.groups.len())
            + &sessions_vm
                .groups
                .iter()
                .flat_map(|g| g.rows.iter().map(|r| r.session_id.as_str()))
                .collect::<Vec<_>>()
                .join("|");
        if self.fp_sessions.as_ref() != Some(&sessions_fp) {
            self.fp_sessions = Some(sessions_fp);
            self.sessions.update(cx, |island, cx| {
                island.apply(sessions_vm, cx);
            });
        }

        let timeline_vm = derive_timeline(self.bridge_state());
        let timeline_fp = format!(
            "{}:{}:{}",
            timeline_vm.rows.len(),
            timeline_vm.rows.last().map(|r| r.body.len()).unwrap_or(0),
            timeline_vm.rows.last().map(|r| r.id.as_str()).unwrap_or("")
        );
        if self.fp_timeline.as_ref() != Some(&timeline_fp) {
            self.fp_timeline = Some(timeline_fp);
            let follow = self.follow_for_selected();
            self.timeline.update(cx, |island, cx| {
                island.apply_timeline(timeline_vm, cx);
                island.set_follow(follow, cx);
            });
        } else {
            let follow = self.follow_for_selected();
            self.timeline.update(cx, |island, cx| {
                island.set_follow(follow, cx);
            });
        }

        let composer_vm = derive_composer(self.bridge_state());
        let activity_vm = derive_activity(self.bridge_state());
        let composer_fp = format!(
            "{}|{}|{}|{}|{}",
            composer_vm.can_send,
            composer_vm.target_label,
            composer_vm.model_label,
            composer_vm.thinking_label,
            activity_vm.items.len()
        );
        if self.fp_composer.as_ref() != Some(&composer_fp) {
            self.fp_composer = Some(composer_fp);
            self.composer.update(cx, |island, cx| {
                island.apply_composer(composer_vm, cx);
                island.apply_activity(activity_vm, cx);
            });
        }

        let agent_vm = derive_agent_tree(self.bridge_state());
        let agents_fp = format!(
            "{:?}|{}",
            island_phase,
            agent_vm
                .nodes
                .iter()
                .map(|n| format!("{}:{}", n.agent_instance_id, n.selected))
                .collect::<Vec<_>>()
                .join("|")
        );
        if self.fp_agents.as_ref() != Some(&agents_fp) {
            self.fp_agents = Some(agents_fp);
            self.agents.update(cx, |island, cx| {
                island.apply(agent_vm, island_phase, cx);
            });
        }

        let expanded = self.tree_expanded_for_selected();
        let tree_vm =
            derive_conversation_tree(self.bridge_state(), self.tree_preview_id(), &expanded);
        let tree_fp = format!(
            "{:?}|{:?}|{}|{}",
            island_phase,
            self.tree_preview_entry_id,
            expanded.len(),
            tree_vm.nodes.len()
        );
        if self.fp_tree.as_ref() != Some(&tree_fp) {
            self.fp_tree = Some(tree_fp);
            self.tree.update(cx, |island, cx| {
                island.apply(tree_vm, island_phase, cx);
            });
        }

        self.apply_island_focus_chrome(cx);
    }

    /// Sync keyboard-focus ring borders onto island Entities.
    pub(crate) fn apply_island_focus_chrome(&mut self, cx: &mut Context<Self>) {
        let focused = self.island_focus.focused();
        self.sessions.update(cx, |i, cx| {
            i.set_chrome_focused(focused == IslandId::Sessions, cx);
        });
        self.timeline.update(cx, |i, cx| {
            i.set_chrome_focused(focused == IslandId::Timeline, cx);
        });
        self.composer.update(cx, |i, cx| {
            i.set_chrome_focused(focused == IslandId::Composer, cx);
        });
        self.agents.update(cx, |i, cx| {
            i.set_chrome_focused(focused == IslandId::Agents, cx);
        });
        self.tree.update(cx, |i, cx| {
            i.set_chrome_focused(focused == IslandId::Tree, cx);
        });
    }

    pub(crate) fn focus_island(
        &mut self,
        id: IslandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.island_focus.set_focused(id);
        self.apply_island_focus_chrome(cx);
        match id {
            IslandId::Sessions => {
                window.focus(&self.sessions.focus_handle(cx));
            }
            IslandId::Timeline => {
                window.focus(&self.timeline.focus_handle(cx));
            }
            IslandId::Composer => {
                window.focus(&self.composer.focus_handle(cx));
                self.composer_input.update(cx, |state, cx| {
                    state.focus(window, cx);
                });
            }
            IslandId::Agents => {
                window.focus(&self.agents.focus_handle(cx));
            }
            IslandId::Tree => {
                window.focus(&self.tree.focus_handle(cx));
            }
        }
    }

    pub(crate) fn visible_focus_islands(&self) -> Vec<IslandId> {
        let live = self.bridge_state().is_live();
        crate::chrome::focus_order(|id| match id {
            IslandId::Timeline | IslandId::Composer => true,
            other => self.layout.is_docked_visible(other, live),
        })
    }
}
