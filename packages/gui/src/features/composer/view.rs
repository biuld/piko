//! Composer island: Entity-owned Activity Center + message input.
//!
use gpui::*;
use gpui_component::input::InputState;

use crate::app::desktop_app::DesktopApp;
use crate::app::island_dispatch::schedule_island_msg;
use crate::shell::{IslandId, IslandMsg, IslandPanel};
use crate::theme::metrics;

use super::ActivityItem;
use super::activity_vm::ActivityViewModel;
use super::render::{render_activity_center, render_composer_panel};
use super::vm::ComposerViewModel;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct ComposerIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    input: Entity<InputState>,
    vm: ComposerViewModel,
    activity: ActivityViewModel,
    activity_expanded: bool,
    activity_user_toggled: bool,
    activity_actionable_fp: String,
}

impl ComposerIsland {
    pub fn new(
        host: WeakEntity<DesktopApp>,
        input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            input,
            vm: ComposerViewModel::default(),
            activity: ActivityViewModel::default(),
            activity_expanded: false,
            activity_user_toggled: false,
            activity_actionable_fp: String::new(),
        }
    }

    pub fn apply_composer(&mut self, vm: ComposerViewModel, cx: &mut Context<Self>) {
        self.vm = vm;
        cx.notify();
    }

    /// Push a freshly derived Activity Center projection. Expansion follows
    /// the same "expand unless the user explicitly collapsed" rule as the
    /// legacy host-owned state (see `app::prompt_host::sync_activity_expand`).
    pub fn apply_activity(&mut self, vm: ActivityViewModel, cx: &mut Context<Self>) {
        let fp = actionable_fingerprint(&vm);
        if fp != self.activity_actionable_fp {
            self.activity_actionable_fp = fp;
            self.activity_user_toggled = false;
            if vm.prefer_expanded {
                self.activity_expanded = true;
            }
        }
        if !vm.prefer_expanded && !self.activity_user_toggled {
            self.activity_expanded = false;
        }
        self.activity = vm;
        cx.notify();
    }

    pub fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.chrome_focused != focused {
            self.chrome_focused = focused;
            cx.notify();
        }
    }

    /// Place keyboard focus when chrome activates this island.
    pub fn take_keyboard_focus(
        &mut self,
        reason: crate::shell::FocusReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if reason != crate::shell::FocusReason::Activate {
            return;
        }
        window.focus(&self.focus_handle);
        self.input.update(cx, |state, cx| {
            state.focus(window, cx);
        });
    }

    fn emit(&self, msg: IslandMsg, window: &mut Window, cx: &mut Context<Self>) {
        schedule_island_msg(self.host.clone(), IslandId::Composer, msg, window, cx);
    }

    fn on_submit(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.emit(IslandMsg::SubmitComposer, window, cx);
    }

    fn on_cancel(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.emit(IslandMsg::CancelTurn, window, cx);
    }

    /// Local expand/collapse toggle; host is also notified via
    /// [`IslandMsg::ToggleActivity`] from the click handler.
    fn on_toggle_activity(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.activity_expanded = !self.activity_expanded;
        self.activity_user_toggled = true;
        self.emit(IslandMsg::ToggleActivity, window, cx);
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Pointer claim: place caret in the composer input, then sync chrome.
        self.take_keyboard_focus(crate::shell::FocusReason::Activate, window, cx);
        self.emit(IslandMsg::ClaimFocus, window, cx);
    }
}

fn actionable_fingerprint(vm: &ActivityViewModel) -> String {
    vm.items
        .iter()
        .filter(|i| i.actionable)
        .map(|i| i.id.as_str())
        .collect::<Vec<_>>()
        .join("|")
}

impl Focusable for ComposerIsland {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ComposerIsland {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let m = metrics();
        let entity = cx.entity().downgrade();

        let on_activity_item = move |item: ActivityItem| -> ClickHandler {
            let entity = entity.clone();
            Box::new(move |_, window, cx| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.emit(
                            IslandMsg::ActivityActivate {
                                agent_instance_id: item.agent_instance_id.clone(),
                                prompt_id: item.prompt_id.clone(),
                            },
                            window,
                            cx,
                        );
                    });
                }
            })
        };

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            // Match Timeline island edge inset: content breathes inside the
            // shell instead of hugging the radius (and stay full-width vs the
            // center column — no outer chrome indent on the island itself).
            .px(m.space_lg)
            .pt(m.space_sm)
            .pb(m.space_md)
            .child(render_activity_center(
                &self.activity,
                self.activity_expanded,
                cx.listener(Self::on_toggle_activity),
                on_activity_item,
            ))
            .child(render_composer_panel(
                &self.vm,
                &self.input,
                cx.listener(Self::on_submit),
                cx.listener(Self::on_cancel),
            ));

        let panel = IslandPanel::new("composer-island", body)
            .scroll(false)
            .fill(false)
            .focused(self.chrome_focused);

        div()
            .id("composer-island-root")
            .track_focus(&self.focus_handle)
            .key_context("IslandComposer")
            .w_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}
