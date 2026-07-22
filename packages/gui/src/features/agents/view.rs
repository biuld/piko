//! Agents island: Entity-owned agent instance tree.
//!
//! List keyboard cursor is [`ListKeyboard`] only (roadmap D5) — no hand-rolled
//! index wrap.

use std::collections::HashSet;

use gpui::*;
use piko_chrome::{ListKeyEffect, ListKeyIntent, ListKeyboard};

use crate::app::desktop_app::DesktopApp;
use crate::app::island_dispatch::schedule_island_msg;
use crate::shell::{IslandId, IslandMsg, IslandSessionPhase, IslandView, activate_focus_handle};

use super::render::{render_agent_tree_panel, visible_agent_ids};
use super::vm::AgentTreeViewModel;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type IdClickFactory = Box<dyn Fn(String) -> ClickHandler>;

actions!(
    agents,
    [
        AgentsSelectPrev,
        AgentsSelectNext,
        AgentsConfirm,
        AgentsToggleExpand
    ]
);

pub struct AgentsIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    vm: AgentTreeViewModel,
    phase: IslandSessionPhase,
    /// Agent ids that are collapsed (absent ⇒ expanded).
    collapsed: HashSet<String>,
    /// Keyboard caret among visible rows (chrome ListKeyboard).
    list_kb: ListKeyboard,
}

impl AgentsIsland {
    pub fn new(host: WeakEntity<DesktopApp>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            vm: AgentTreeViewModel::default(),
            phase: IslandSessionPhase::Idle,
            collapsed: HashSet::new(),
            list_kb: ListKeyboard::new(),
        }
    }

    /// Push a freshly derived agent-tree projection plus session phase.
    pub fn apply(
        &mut self,
        vm: AgentTreeViewModel,
        phase: IslandSessionPhase,
        cx: &mut Context<Self>,
    ) {
        self.vm = vm;
        self.phase = phase;
        let visible = visible_agent_ids(&self.vm, &self.collapsed);
        self.list_kb.sync_len(visible.len());
        cx.notify();
    }

    fn emit(&self, msg: IslandMsg, window: &mut Window, cx: &mut Context<Self>) {
        schedule_island_msg(self.host.clone(), IslandId::Agents, msg, window, cx);
    }

    fn toggle_expand(&mut self, agent_id: String, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&agent_id) {
            self.collapsed.insert(agent_id);
        }
        // Visible length may change after collapse/expand.
        let visible = visible_agent_ids(&self.vm, &self.collapsed);
        self.list_kb.sync_len(visible.len());
        cx.notify();
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.emit(IslandMsg::ClaimFocus, window, cx);
    }

    fn select_prev(&mut self, _: &AgentsSelectPrev, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Prev, window, cx);
    }

    fn select_next(&mut self, _: &AgentsSelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Next, window, cx);
    }

    fn confirm(&mut self, _: &AgentsConfirm, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Activate, window, cx);
    }

    fn toggle_focused(
        &mut self,
        _: &AgentsToggleExpand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_list_key(ListKeyIntent::ToggleExpand, window, cx);
    }

    fn apply_list_key(
        &mut self,
        intent: ListKeyIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let visible = visible_agent_ids(&self.vm, &self.collapsed);
        let len = visible.len();
        match self.list_kb.apply(len, intent) {
            ListKeyEffect::None => {}
            ListKeyEffect::CursorMoved { .. } => {
                cx.notify();
            }
            ListKeyEffect::Activate { index } => {
                if let Some(id) = visible.get(index).cloned() {
                    self.emit(
                        IslandMsg::SelectAgent {
                            agent_instance_id: id,
                        },
                        window,
                        cx,
                    );
                }
                cx.notify();
            }
            ListKeyEffect::ToggleExpand { index } => {
                if let Some(id) = visible.get(index).cloned()
                    && self
                        .vm
                        .nodes
                        .iter()
                        .any(|n| n.agent_instance_id == id && n.has_children)
                {
                    self.toggle_expand(id, cx);
                    return;
                }
                cx.notify();
            }
        }
    }

    fn preferred_keyboard_index(&self, visible: &[String]) -> Option<usize> {
        visible.iter().position(|id| {
            self.vm
                .nodes
                .iter()
                .any(|n| n.agent_instance_id == *id && n.selected)
        })
    }
}

impl IslandView for AgentsIsland {
    type Id = IslandId;

    fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.chrome_focused != focused {
            self.chrome_focused = focused;
            if focused {
                let visible = visible_agent_ids(&self.vm, &self.collapsed);
                let preferred = self.preferred_keyboard_index(&visible);
                self.list_kb.ensure_cursor(visible.len(), preferred);
            }
            cx.notify();
        }
    }

    fn take_keyboard_focus(
        &mut self,
        reason: crate::shell::FocusReason,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        activate_focus_handle(&self.focus_handle, reason, window);
    }
}

impl Focusable for AgentsIsland {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AgentsIsland {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();

        let on_select: IdClickFactory = Box::new({
            let entity = entity.clone();
            move |agent_instance_id| {
                let entity = entity.clone();
                Box::new(move |_, window, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.emit(
                                IslandMsg::SelectAgent {
                                    agent_instance_id: agent_instance_id.clone(),
                                },
                                window,
                                cx,
                            );
                        });
                    }
                })
            }
        });

        let on_toggle: IdClickFactory = Box::new({
            let entity = entity.clone();
            move |agent_id| {
                let entity = entity.clone();
                Box::new(move |_, _window, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.toggle_expand(agent_id.clone(), cx);
                        });
                    }
                })
            }
        });

        let visible = visible_agent_ids(&self.vm, &self.collapsed);
        let keyboard_id = self
            .list_kb
            .cursor()
            .and_then(|ix| visible.get(ix).map(|s| s.as_str()))
            .filter(|_| self.chrome_focused);

        let panel = render_agent_tree_panel(
            &self.vm,
            &self.collapsed,
            self.phase,
            self.chrome_focused,
            keyboard_id,
            on_select,
            on_toggle,
        );

        div()
            .id("agents-island-root")
            .track_focus(&self.focus_handle)
            .key_context("IslandAgents")
            .size_full()
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::toggle_focused))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}
