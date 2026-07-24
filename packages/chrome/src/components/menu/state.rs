//! Stateful flat context-menu Entity.

use gpui::{prelude::FluentBuilder, *};

use super::item::{ContextMenuCallback, ContextMenuItemKind};
use super::navigation::step;
use super::registry::ContextMenuRegistry;
use super::{ContextMenuItem, ContextMenuSpec};
use crate::theme::{RoleAccent, TextRole, text, tokens};

const KEY_CONTEXT: &str = "PikoContextMenu";

actions!(
    piko_context_menu,
    [SelectPrevious, SelectNext, ConfirmSelection, DismissMenu]
);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, Some(KEY_CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", ConfirmSelection, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", DismissMenu, Some(KEY_CONTEXT)),
    ]);
}

pub struct ContextMenu {
    focus_handle: FocusHandle,
    items: Vec<ContextMenuItem>,
    selected: Option<usize>,
    max_width: Pixels,
}

impl ContextMenu {
    pub(crate) fn new(spec: ContextMenuSpec, max_width: Pixels, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            items: spec.items,
            selected: None,
            max_width,
        }
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.selected = step(&self.items, self.selected, -1);
        cx.notify();
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.selected = step(&self.items, self.selected, 1);
        cx.notify();
    }

    fn confirm(&mut self, _: &ConfirmSelection, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(callback) = self
            .selected
            .and_then(|index| self.items.get(index))
            .and_then(ContextMenuItem::callback)
        {
            self.finish(Some(callback), window, cx);
        }
    }

    fn dismiss(&mut self, _: &DismissMenu, window: &mut Window, cx: &mut Context<Self>) {
        self.finish(None, window, cx);
    }

    pub(crate) fn replace(&mut self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn finish(
        &mut self,
        callback: Option<ContextMenuCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let window_id = window.window_handle().window_id();
        let entity_id = cx.entity_id();
        let restore_focus = cx.update_default_global::<ContextMenuRegistry, _>(|registry, _| {
            let matches = registry
                .windows
                .get(&window_id)
                .is_some_and(|active| active.entity_id == entity_id);
            matches
                .then(|| registry.windows.remove(&window_id))
                .flatten()
                .and_then(|active| active.restore_focus)
        });
        cx.emit(DismissEvent);
        if let Some(restore_focus) = restore_focus {
            restore_focus.focus(window);
        }
        if let Some(callback) = callback {
            callback(window, cx);
        }
    }

    fn invoke(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(callback) = self.items.get(index).and_then(ContextMenuItem::callback) {
            self.selected = Some(index);
            self.finish(Some(callback), window, cx);
        }
    }
}

impl EventEmitter<DismissEvent> for ContextMenu {}

impl Focusable for ContextMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ContextMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens();
        let min_width = px(144.).min(self.max_width);
        div()
            .id("piko-context-menu")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .min_w(min_width)
            .max_w(self.max_width)
            .p(px(4.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .rounded(px(8.))
            .border_1()
            .border_color(t.border_rgba())
            .bg(t.elevated_rgba())
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::dismiss))
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.finish(None, window, cx);
            }))
            .children(
                self.items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| self.render_item(index, item, cx)),
            )
    }
}

impl ContextMenu {
    fn render_item(
        &self,
        index: usize,
        item: &ContextMenuItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = tokens();
        match &item.kind {
            ContextMenuItemKind::Separator => div()
                .h(px(1.))
                .mx(px(4.))
                .my(px(2.))
                .bg(t.border_rgba())
                .into_any_element(),
            ContextMenuItemKind::Action {
                label,
                enabled,
                tone,
                ..
            } => {
                let selected = self.selected == Some(index);
                let foreground = if !enabled {
                    t.muted_fg_rgba()
                } else if *tone == super::ContextMenuItemTone::Destructive {
                    t.role_accent(RoleAccent::Danger)
                } else {
                    t.fg_rgba()
                };
                let mut row = div()
                    .id(("piko-context-menu-item", index))
                    .h(px(32.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .when(selected, |row| row.bg(t.border_rgba()))
                    .child(
                        text(TextRole::Label)
                            .text_color(foreground)
                            .child(label.clone()),
                    );
                if *enabled {
                    row = row
                        .cursor_pointer()
                        .hover(|style| style.bg(t.border_rgba()))
                        .on_hover(cx.listener(move |this, hovered, _, cx| {
                            if *hovered && this.selected != Some(index) {
                                this.selected = Some(index);
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.invoke(index, window, cx);
                        }));
                }
                row.into_any_element()
            }
        }
    }
}
