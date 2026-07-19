//! DesktopApp layout chrome: panes, sheets, and tree navigation.

use std::collections::{HashMap, HashSet};

use gpui::*;
use gpui_component::{Placement, WindowExt};

use crate::chrome::IslandId;
use crate::islands::{
    default_tree_expansion, derive_conversation_tree, derive_timeline, prune_tree_expansion,
};

use super::desktop_app::{DesktopApp, ToggleRightColumn, ToggleSessions};

impl DesktopApp {
    pub(crate) fn tree_preview_id(&self) -> Option<&str> {
        self.tree_preview_entry_id.as_deref()
    }

    pub(crate) fn sync_layout_breakpoint(&mut self, window: &Window) {
        let width: f32 = window.viewport_size().width.into();
        self.layout.sync_window_width(width);
    }

    pub(crate) fn action_toggle_sessions(
        &mut self,
        _: &ToggleSessions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let live = self.bridge_state().is_live();
        if self.layout.is_docked_visible(IslandId::Sessions, live) {
            self.layout.set_open(IslandId::Sessions, false);
            self.persist_gui_config();
            cx.notify();
            return;
        }
        self.layout.set_open(IslandId::Sessions, true);
        self.persist_gui_config();
        if self.layout.is_docked_visible(IslandId::Sessions, live) {
            cx.notify();
        } else {
            self.open_sessions_sheet(window, cx);
        }
    }

    pub(crate) fn action_toggle_right_column(
        &mut self,
        _: &ToggleRightColumn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let live = self.bridge_state().is_live();
        if self.layout.any_right_column_docked(live) {
            self.layout.set_right_column_open(false);
            self.persist_gui_config();
            cx.notify();
            return;
        }
        self.layout.set_right_column_open(true);
        self.persist_gui_config();
        if self.layout.any_right_column_docked(live) {
            cx.notify();
        } else {
            self.open_right_column_sheet(window, cx);
        }
    }

    pub(crate) fn open_sessions_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.layout.close_session_sheet_on_live = true;
        self.island_focus.save_and_focus(IslandId::Sessions);
        self.apply_island_focus_chrome(cx);
        let sessions = self.sessions.clone();
        window.open_sheet_at(Placement::Left, cx, move |sheet, _window, _cx| {
            sheet
                .title(crate::t!("sheet.sessions.title"))
                .child(sessions.clone())
        });
        self.focus_island(IslandId::Sessions, window, cx);
    }

    pub(crate) fn open_right_column_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.island_focus.save_and_focus(IslandId::Agents);
        self.apply_island_focus_chrome(cx);
        let agents_height = self.layout.agents_height;
        let entity = cx.entity().downgrade();
        let agents = self.agents.clone();
        let tree = self.tree.clone();
        window.open_sheet_at(Placement::Right, cx, move |sheet, _window, _cx| {
            let entity_resize = entity.clone();
            sheet.child(crate::chrome::render_right_column(
                agents.clone(),
                tree.clone(),
                true,
                true,
                agents_height,
                Box::new(move |height, cx| {
                    if let Some(view) = entity_resize.upgrade() {
                        view.update(cx, |this, _| {
                            this.layout.agents_height = height;
                            this.persist_gui_config();
                        });
                    }
                }),
            ))
        });
        self.focus_island(IslandId::Agents, window, cx);
    }

    pub(crate) fn maybe_close_session_sheet_on_live(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.layout.close_session_sheet_on_live && self.bridge_state().is_live() {
            self.layout.close_session_sheet_on_live = false;
            if window.has_active_sheet(cx) {
                window.close_sheet(cx);
                self.island_focus.restore();
                self.apply_island_focus_chrome(cx);
            }
        }
    }

    pub(crate) fn handle_tree_activate(
        &mut self,
        entry_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let all_expanded = self.seed_full_expansion();
        let tree = derive_conversation_tree(self.bridge_state(), Some(&entry_id), &all_expanded);
        let Some(node) = tree.nodes.iter().find(|n| n.id == entry_id) else {
            return;
        };
        if node.on_path {
            self.tree_preview_entry_id = None;
            self.pending_timeline_scroll_id = Some(entry_id);
            self.apply_pending_timeline_scroll(cx);
        } else {
            self.tree_preview_entry_id = Some(entry_id);
        }
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn handle_tree_toggle_expand(&mut self, entry_id: String, cx: &mut Context<Self>) {
        let agent = self.selected_agent_id_pub().unwrap_or_else(|| "_".into());
        let seed = self.default_expansion_seed();
        let set = self.tree_expanded_by_agent.entry(agent).or_insert(seed);
        if !set.remove(&entry_id) {
            set.insert(entry_id);
        }
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn apply_pending_timeline_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_timeline_scroll_id.clone() else {
            return;
        };
        let timeline = derive_timeline(self.bridge_state());
        if let Some(ix) = timeline.rows.iter().position(|r| r.id == id) {
            self.timeline.update(cx, |island, cx| {
                island.scroll_to_item(ix);
                island.set_follow(false, cx);
            });
            self.pending_timeline_scroll_id = None;
            if let Some(agent) = self.selected_agent_id_pub() {
                self.follow_bottom.insert(agent, false);
            }
        }
    }

    pub(crate) fn tree_expanded_for_selected(&self) -> HashSet<String> {
        let agent = self.selected_agent_id_pub().unwrap_or_else(|| "_".into());
        self.tree_expanded_by_agent
            .get(&agent)
            .cloned()
            .unwrap_or_else(|| self.default_expansion_seed())
    }

    fn default_expansion_seed(&self) -> HashSet<String> {
        let Some(session) = self.bridge_state().live_session.as_ref() else {
            return HashSet::new();
        };
        default_tree_expansion(
            &session.entries,
            session.current_leaf_id.as_deref(),
            session.selected_agent.as_deref(),
        )
    }

    fn seed_full_expansion(&self) -> HashSet<String> {
        let Some(session) = self.bridge_state().live_session.as_ref() else {
            return HashSet::new();
        };
        session.entries.iter().map(|e| e.id().to_string()).collect()
    }

    fn selected_agent_id_pub(&self) -> Option<String> {
        self.bridge_state()
            .live_session
            .as_ref()
            .and_then(|s| s.selected_agent.clone())
    }

    pub(crate) fn prune_map_state_after_reconcile(&mut self) {
        let Some(session) = self.bridge_state().live_session.as_ref() else {
            self.tree_preview_entry_id = None;
            self.tree_expanded_by_agent.clear();
            return;
        };
        let surviving: HashSet<String> =
            session.entries.iter().map(|e| e.id().to_string()).collect();
        let leaf = session.current_leaf_id.clone();
        let selected_agent = session.selected_agent.clone();
        let seed =
            default_tree_expansion(&session.entries, leaf.as_deref(), selected_agent.as_deref());

        if let Some(preview) = self.tree_preview_entry_id.clone()
            && (!surviving.contains(&preview) || leaf.as_deref() == Some(preview.as_str()))
        {
            self.tree_preview_entry_id = None;
        }

        for set in self.tree_expanded_by_agent.values_mut() {
            prune_tree_expansion(set, &surviving);
        }

        if let Some(agent) = selected_agent {
            self.tree_expanded_by_agent.entry(agent).or_insert(seed);
        }
    }
}

pub(crate) type TreeExpandedByAgent = HashMap<String, HashSet<String>>;
