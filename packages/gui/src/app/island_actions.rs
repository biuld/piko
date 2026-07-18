//! DesktopApp layout chrome: panes, sheets, tree navigation, branch switch.

use std::collections::{HashMap, HashSet};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Placement, WindowExt};
use piko_client_core::ClientIntent;

use crate::chrome::IslandId;
use crate::islands::{
    default_tree_expansion, derive_conversation_tree, derive_timeline, prune_tree_expansion,
};

use super::desktop_app::{DesktopApp, ToggleAgentsTree, ToggleSessions};

impl DesktopApp {
    pub(crate) fn layout(&self) -> &super::layout_state::LayoutState {
        &self.layout
    }

    pub(crate) fn tree_preview_id(&self) -> Option<&str> {
        self.tree_preview_entry_id.as_deref()
    }

    pub(crate) fn sync_layout_breakpoint(&mut self, window: &Window) {
        let width: f32 = window.viewport_size().width.into();
        self.layout.sync_breakpoint(width);
    }

    pub(crate) fn action_toggle_sessions(
        &mut self,
        _: &ToggleSessions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.layout.breakpoint {
            super::layout_state::LayoutBreakpoint::Narrow => {
                self.open_sessions_sheet(window, cx);
            }
            _ => {
                self.layout.toggle(IslandId::Sessions);
                self.persist_gui_config();
                cx.notify();
            }
        }
    }

    pub(crate) fn action_toggle_agents_tree(
        &mut self,
        _: &ToggleAgentsTree,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.layout.breakpoint {
            super::layout_state::LayoutBreakpoint::Wide => {
                self.layout.toggle_agents_tree();
                self.persist_gui_config();
                cx.notify();
            }
            _ => {
                self.open_agents_tree_sheet(window, cx);
            }
        }
    }

    pub(crate) fn open_sessions_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.layout.close_session_sheet_on_live = true;
        self.island_focus.save_and_focus(IslandId::Sessions);
        self.apply_island_focus_chrome(cx);
        let sessions = self.sessions.clone();
        window.open_sheet_at(Placement::Left, cx, move |sheet, _window, _cx| {
            sheet.title("Sessions").child(sessions.clone())
        });
        self.focus_island(IslandId::Sessions, window, cx);
    }

    pub(crate) fn open_agents_tree_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.island_focus.save_and_focus(IslandId::Agents);
        self.apply_island_focus_chrome(cx);
        let agents_height = self.layout.agents_height;
        let entity = cx.entity().downgrade();
        let agents = self.agents.clone();
        let tree = self.tree.clone();
        window.open_sheet_at(Placement::Right, cx, move |sheet, _window, _cx| {
            let entity_resize = entity.clone();
            sheet.child(crate::chrome::render_agents_tree_entities(
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

    pub(crate) fn confirm_switch_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry_id) = self.tree_preview_entry_id.clone() else {
            return;
        };
        let entity = cx.entity().downgrade();
        let entry_for_ok = entry_id.clone();
        let m = crate::theme::metrics();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .title("Switch Branch")
                .overlay_closable(false)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(m.space_sm)
                        .child(
                            div()
                                .text_size(m.body_size)
                                .line_height(m.body_line_height)
                                .child(
                                    "Navigate the session to the selected entry? The current branch will be summarized.",
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(m.space_sm)
                                .child(
                                    Button::new("branch-confirm")
                                        .primary()
                                        .label("Switch")
                                        .on_click({
                                            let entity = entity.clone();
                                            let entry_id = entry_for_ok.clone();
                                            move |_, window, cx| {
                                                if let Some(view) = entity.upgrade() {
                                                    view.update(cx, |this, cx| {
                                                        this.bridge_mut().intent(
                                                            ClientIntent::NavigateSession {
                                                                entry_id: entry_id.clone(),
                                                                summarize: true,
                                                                custom_instructions: None,
                                                            },
                                                        );
                                                        this.tree_preview_entry_id = None;
                                                        window.close_dialog(cx);
                                                        this.refresh_islands(cx);
                                                        cx.notify();
                                                    });
                                                }
                                            }
                                        }),
                                )
                                .child(Button::new("branch-cancel").label("Cancel").on_click(
                                    |_, window, cx| {
                                        window.close_dialog(cx);
                                    },
                                )),
                        ),
                )
                .footer(|_, _, _, _| Vec::<AnyElement>::new())
                .on_cancel(|_, _, _| true)
        });
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
        default_tree_expansion(&session.entries, session.current_leaf_id.as_deref())
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
        let seed = default_tree_expansion(&session.entries, leaf.as_deref());

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

pub(crate) fn render_pane_toggles(
    app: &DesktopApp,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let show_session = app.layout().is_docked_visible(IslandId::Sessions, true);
    let show_agents_tree = app.layout().any_agents_tree_docked(true);
    let need_bar = !show_session || !show_agents_tree;
    let t = crate::theme::tokens();
    let m = crate::theme::metrics();
    div().when(need_bar, |d| {
        d.h(m.compact_bar_height)
            .px(m.space_sm)
            .flex()
            .items_center()
            .gap(m.space_sm)
            .border_b_1()
            .border_color(t.border_rgba())
            .when(!show_session, |d| {
                let entity = entity.clone();
                d.child(
                    Button::new("open-sessions")
                        .label("Sessions")
                        .on_click(move |_, window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.action_toggle_sessions(&ToggleSessions, window, cx);
                                });
                            }
                        })
                        .ghost()
                        .small()
                        .compact(),
                )
            })
            .when(!show_agents_tree, |d| {
                let entity = entity.clone();
                d.child(
                    Button::new("open-agents-tree")
                        .label("Agents")
                        .on_click(move |_, window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.action_toggle_agents_tree(&ToggleAgentsTree, window, cx);
                                });
                            }
                        })
                        .ghost()
                        .small()
                        .compact(),
                )
            })
    })
}

pub(crate) type TreeExpandedByAgent = HashMap<String, HashSet<String>>;
