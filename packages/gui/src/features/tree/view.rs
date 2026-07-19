//! Tree island: Entity-owned conversation tree (display / navigation preview).
//!
use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::app::island_dispatch::schedule_island_msg;
use crate::shell::{IslandId, IslandMsg, IslandSessionPhase};

use super::render::render_tree_panel;
use super::vm::ConversationTreeViewModel;

type IdClickFactory =
    Box<dyn Fn(String) -> Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>;

pub struct TreeIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    vm: ConversationTreeViewModel,
    phase: IslandSessionPhase,
}

impl TreeIsland {
    pub fn new(host: WeakEntity<DesktopApp>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            vm: ConversationTreeViewModel::default(),
            phase: IslandSessionPhase::Idle,
        }
    }

    /// Push a freshly derived tree projection (preview + expansion already
    /// resolved by the host) plus session phase.
    pub fn apply(
        &mut self,
        vm: ConversationTreeViewModel,
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
        schedule_island_msg(self.host.clone(), IslandId::Tree, msg, window, cx);
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.emit(IslandMsg::ClaimFocus, window, cx);
    }
}

impl Focusable for TreeIsland {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TreeIsland {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();

        let on_tree_activate: IdClickFactory = Box::new({
            let entity = entity.clone();
            move |entry_id| {
                let entity = entity.clone();
                Box::new(move |_, window, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.emit(
                                IslandMsg::TreeActivate {
                                    entry_id: entry_id.clone(),
                                },
                                window,
                                cx,
                            );
                        });
                    }
                })
            }
        });

        let on_tree_toggle_expand: IdClickFactory = Box::new({
            let entity = entity.clone();
            move |entry_id| {
                let entity = entity.clone();
                Box::new(move |_, window, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.emit(
                                IslandMsg::TreeToggleExpand {
                                    entry_id: entry_id.clone(),
                                },
                                window,
                                cx,
                            );
                        });
                    }
                })
            }
        });

        let panel = render_tree_panel(
            &self.vm,
            self.phase,
            self.chrome_focused,
            on_tree_activate,
            on_tree_toggle_expand,
        );

        div()
            .id("tree-island-root")
            .track_focus(&self.focus_handle)
            .key_context("IslandTree")
            .size_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}
