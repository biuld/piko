//! Agents island: Entity-owned agent instance tree.
//!
use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::app::island_dispatch::schedule_island_msg;
use crate::chrome::{IslandId, IslandMsg, IslandSessionPhase};

use super::render::render_agent_tree_panel;
use super::vm::AgentTreeViewModel;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct AgentsIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    vm: AgentTreeViewModel,
    phase: IslandSessionPhase,
}

impl AgentsIsland {
    pub fn new(host: WeakEntity<DesktopApp>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            vm: AgentTreeViewModel::default(),
            phase: IslandSessionPhase::Idle,
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
        cx.notify();
    }

    pub fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.chrome_focused != focused {
            self.chrome_focused = focused;
            cx.notify();
        }
    }

    fn emit(&self, msg: IslandMsg, window: &mut Window, cx: &mut Context<Self>) {
        schedule_island_msg(self.host.clone(), IslandId::Agents, msg, window, cx);
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.emit(IslandMsg::ClaimFocus, window, cx);
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

        let on_select = move |agent_instance_id: String| -> ClickHandler {
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
        };

        let panel = render_agent_tree_panel(&self.vm, self.phase, self.chrome_focused, on_select);

        div()
            .id("agents-island-root")
            .track_focus(&self.focus_handle)
            .key_context("IslandAgents")
            .size_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}
