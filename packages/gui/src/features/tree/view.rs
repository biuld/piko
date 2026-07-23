//! Tree island: Entity-owned conversation tree (display / navigation preview).
//!
use gpui::*;
use piko_chrome::{ListKeyEffect, ListKeyIntent, ListKeyboard};

use crate::app::desktop_app::DesktopApp;
use crate::app::island_dispatch::schedule_island_msg;
use crate::shell::{IslandId, IslandMsg, IslandSessionPhase, IslandView, activate_focus_handle};

use super::render::render_tree_panel;
use super::vm::ConversationTreeViewModel;

actions!(
    tree,
    [
        TreeSelectPrev,
        TreeSelectNext,
        TreeConfirm,
        TreeToggleFocused
    ]
);

type IdClickFactory =
    Box<dyn Fn(String) -> Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>;

pub struct TreeIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    vm: ConversationTreeViewModel,
    phase: IslandSessionPhase,
    list_kb: ListKeyboard,
}

impl TreeIsland {
    pub fn new(host: WeakEntity<DesktopApp>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            vm: ConversationTreeViewModel::default(),
            phase: IslandSessionPhase::Idle,
            list_kb: ListKeyboard::new(),
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
        self.list_kb.sync_len(self.vm.nodes.len());
        cx.notify();
    }

    fn emit(&self, msg: IslandMsg, window: &mut Window, cx: &mut Context<Self>) {
        schedule_island_msg(self.host.clone(), IslandId::Tree, msg, window, cx);
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.emit(IslandMsg::ClaimFocus, window, cx);
    }

    fn select_prev(&mut self, _: &TreeSelectPrev, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Prev, window, cx);
    }

    fn select_next(&mut self, _: &TreeSelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Next, window, cx);
    }

    fn confirm(&mut self, _: &TreeConfirm, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Activate, window, cx);
    }

    fn toggle_focused(
        &mut self,
        _: &TreeToggleFocused,
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
        match self.list_kb.apply(self.vm.nodes.len(), intent) {
            ListKeyEffect::None => {}
            ListKeyEffect::CursorMoved { .. } => cx.notify(),
            ListKeyEffect::Activate { index } => {
                if let Some(node) = self.vm.nodes.get(index) {
                    self.emit(
                        IslandMsg::TreeActivate {
                            entry_id: node.id.clone(),
                        },
                        window,
                        cx,
                    );
                }
                cx.notify();
            }
            ListKeyEffect::ToggleExpand { index } => {
                if let Some(node) = self.vm.nodes.get(index)
                    && node.has_children
                {
                    self.emit(
                        IslandMsg::TreeToggleExpand {
                            entry_id: node.id.clone(),
                        },
                        window,
                        cx,
                    );
                }
                cx.notify();
            }
        }
    }
}

impl IslandView for TreeIsland {
    type Id = IslandId;

    fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.chrome_focused != focused {
            self.chrome_focused = focused;
            if focused {
                let preferred = self.vm.nodes.iter().position(|node| {
                    self.vm.preview_entry_id.as_deref() == Some(node.id.as_str())
                        || self.vm.current_leaf_id.as_deref() == Some(node.id.as_str())
                });
                self.list_kb.ensure_cursor(self.vm.nodes.len(), preferred);
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
            self.list_kb.cursor().filter(|_| self.chrome_focused),
            on_tree_activate,
            on_tree_toggle_expand,
        );

        div()
            .id("tree-island-root")
            .track_focus(&self.focus_handle)
            .key_context("IslandTree")
            .size_full()
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::toggle_focused))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}
