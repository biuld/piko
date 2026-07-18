//! DesktopApp layout chrome: panes, sheets, map navigation, branch switch.

use std::collections::{HashMap, HashSet};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Placement, WindowExt};
use piko_client_core::ClientIntent;

use crate::inspector::{
    InspectorHandlers, InspectorSheetHandlers, default_map_expansion, derive_conversation_map,
    prune_expansion, render_inspector_sheet_body,
};
use crate::workbench::derive_agent_tree;
use crate::workbench::derive_timeline;

use super::desktop_app::{DesktopApp, ToggleInspector, ToggleSessions};
use super::layout_state::InspectorTab;
use super::session_sidebar::render_session_sidebar;

impl DesktopApp {
    pub(crate) fn layout(&self) -> &super::layout_state::LayoutState {
        &self.layout
    }

    pub(crate) fn map_preview_id(&self) -> Option<&str> {
        self.map_preview_entry_id.as_deref()
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
                self.layout.toggle_session_pref();
                self.persist_gui_config();
                cx.notify();
            }
        }
    }

    pub(crate) fn action_toggle_inspector(
        &mut self,
        _: &ToggleInspector,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.bridge_state().is_live() {
            return;
        }
        match self.layout.breakpoint {
            super::layout_state::LayoutBreakpoint::Wide => {
                self.layout.toggle_inspector_pref();
                self.persist_gui_config();
                cx.notify();
            }
            _ => {
                self.open_inspector_sheet(window, cx);
            }
        }
    }

    pub(crate) fn open_sessions_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.layout.close_session_sheet_on_live = true;
        let entity = cx.entity().downgrade();
        let state_snapshot = self.bridge_state().clone();
        window.open_sheet_at(Placement::Left, cx, move |sheet, _window, _cx| {
            let entity = entity.clone();
            sheet
                .title("Sessions")
                .child(render_session_sidebar(&state_snapshot, entity))
        });
    }

    pub(crate) fn open_inspector_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let agent_tree = derive_agent_tree(self.bridge_state());
        let expanded = self.map_expanded_for_selected();
        let map = derive_conversation_map(self.bridge_state(), self.map_preview_id(), &expanded);
        let tab = self.layout.inspector_tab;
        let entity = cx.entity().downgrade();

        window.open_sheet_at(Placement::Right, cx, move |sheet, _window, _cx| {
            let entity_agents = entity.clone();
            let entity_map = entity.clone();
            sheet.title("Inspector").child(render_inspector_sheet_body(
                tab,
                &agent_tree,
                &map,
                InspectorSheetHandlers {
                    on_tab_agents: Box::new(move |_, w, cx| {
                        if let Some(view) = entity_agents.upgrade() {
                            view.update(cx, |this, cx| {
                                this.layout.inspector_tab = InspectorTab::Agents;
                                this.open_inspector_sheet(w, cx);
                            });
                        }
                    }),
                    on_tab_map: Box::new(move |_, w, cx| {
                        if let Some(view) = entity_map.upgrade() {
                            view.update(cx, |this, cx| {
                                this.layout.inspector_tab = InspectorTab::Map;
                                this.open_inspector_sheet(w, cx);
                            });
                        }
                    }),
                    panel: inspector_handlers(entity.clone()),
                },
            ))
        });
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
            }
        }
    }

    pub(crate) fn handle_map_activate(
        &mut self,
        entry_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let all_expanded = self.seed_full_expansion();
        let map = derive_conversation_map(self.bridge_state(), Some(&entry_id), &all_expanded);
        let Some(node) = map.nodes.iter().find(|n| n.id == entry_id) else {
            return;
        };
        if node.on_path {
            self.map_preview_entry_id = None;
            self.pending_timeline_scroll_id = Some(entry_id);
            self.apply_pending_timeline_scroll();
        } else {
            self.map_preview_entry_id = Some(entry_id);
        }
        cx.notify();
    }

    pub(crate) fn handle_map_toggle_expand(&mut self, entry_id: String, cx: &mut Context<Self>) {
        let agent = self.selected_agent_id_pub().unwrap_or_else(|| "_".into());
        let seed = self.default_expansion_seed();
        let set = self.map_expanded_by_agent.entry(agent).or_insert(seed);
        if !set.remove(&entry_id) {
            set.insert(entry_id);
        }
        cx.notify();
    }

    pub(crate) fn apply_pending_timeline_scroll(&mut self) {
        let Some(id) = self.pending_timeline_scroll_id.clone() else {
            return;
        };
        let timeline = derive_timeline(self.bridge_state());
        if let Some(ix) = timeline.rows.iter().position(|r| r.id == id) {
            self.timeline_scroll.scroll_to_item(ix);
            self.pending_timeline_scroll_id = None;
            if let Some(agent) = self.selected_agent_id_pub() {
                self.follow_bottom.insert(agent, false);
            }
        }
    }

    pub(crate) fn confirm_switch_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry_id) = self.map_preview_entry_id.clone() else {
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
                                                        this.map_preview_entry_id = None;
                                                        window.close_dialog(cx);
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

    pub(crate) fn map_expanded_for_selected(&self) -> HashSet<String> {
        let agent = self.selected_agent_id_pub().unwrap_or_else(|| "_".into());
        self.map_expanded_by_agent
            .get(&agent)
            .cloned()
            .unwrap_or_else(|| self.default_expansion_seed())
    }

    fn default_expansion_seed(&self) -> HashSet<String> {
        let Some(session) = self.bridge_state().live_session.as_ref() else {
            return HashSet::new();
        };
        default_map_expansion(&session.entries, session.current_leaf_id.as_deref())
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
            self.map_preview_entry_id = None;
            self.map_expanded_by_agent.clear();
            return;
        };
        let surviving: HashSet<String> =
            session.entries.iter().map(|e| e.id().to_string()).collect();
        let leaf = session.current_leaf_id.clone();
        let selected_agent = session.selected_agent.clone();
        let seed = default_map_expansion(&session.entries, leaf.as_deref());

        if let Some(preview) = self.map_preview_entry_id.clone()
            && (!surviving.contains(&preview) || leaf.as_deref() == Some(preview.as_str()))
        {
            self.map_preview_entry_id = None;
        }

        for set in self.map_expanded_by_agent.values_mut() {
            prune_expansion(set, &surviving);
        }

        if let Some(agent) = selected_agent {
            self.map_expanded_by_agent.entry(agent).or_insert(seed);
        }
    }
}

pub(crate) fn inspector_handlers(entity: WeakEntity<DesktopApp>) -> InspectorHandlers {
    InspectorHandlers {
        on_select_agent: Box::new({
            let entity = entity.clone();
            move |agent_id| {
                let entity = entity.clone();
                Box::new(move |_, window, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.handle_select_agent(agent_id.clone(), window, cx);
                        });
                    }
                })
            }
        }),
        on_map_activate: Box::new({
            let entity = entity.clone();
            move |entry_id| {
                let entity = entity.clone();
                Box::new(move |_, window, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.handle_map_activate(entry_id.clone(), window, cx);
                        });
                    }
                })
            }
        }),
        on_map_toggle_expand: Box::new({
            let entity = entity.clone();
            move |entry_id| {
                let entity = entity.clone();
                Box::new(move |_, _, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.handle_map_toggle_expand(entry_id.clone(), cx);
                        });
                    }
                })
            }
        }),
        on_switch_branch: Box::new({
            let entity = entity.clone();
            move |_, window, cx| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.confirm_switch_branch(window, cx);
                    });
                }
            }
        }),
    }
}

pub(crate) fn render_pane_toggles(
    app: &DesktopApp,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let show_session = app.layout().show_session_pane();
    let live = app.bridge_state().is_live();
    let show_inspector = app.layout().show_inspector_pane(live);
    let need_bar = !show_session || (live && !show_inspector);
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
            .when(live && !show_inspector, |d| {
                let entity = entity.clone();
                d.child(
                    Button::new("open-inspector")
                        .label("Inspector")
                        .on_click(move |_, window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.action_toggle_inspector(&ToggleInspector, window, cx);
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

/// Ensure DesktopApp declares the expansion map field (used from `new`).
pub(crate) type MapExpandedByAgent = HashMap<String, HashSet<String>>;
