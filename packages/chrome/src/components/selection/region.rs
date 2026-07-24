//! Mouse, keyboard, and context-menu behavior for one selectable row.

use gpui::*;

use crate::components::menu::{ContextMenuExt, ContextMenuItem, ContextMenuSpec};

use super::SelectionState;

const KEY_CONTEXT: &str = "PikoSelectionRegion";

actions!(piko_selection, [CopySelection]);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, Some(KEY_CONTEXT))]);
}

pub fn selectable_region(
    id: impl Into<ElementId>,
    state: Entity<SelectionState>,
    copy_label: impl Into<SharedString>,
    child: impl IntoElement,
    notify_owner: EntityId,
    cx: &mut App,
) -> AnyElement {
    state.update(cx, |state, _| state.begin_frame());
    let focus = state.read(cx).focus_handle();
    let copy_label = copy_label.into();
    let copy_state = state.clone();
    div()
        .id(id)
        .key_context(KEY_CONTEXT)
        .track_focus(&focus)
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(MouseButton::Left, {
            let state = state.clone();
            move |event, window, cx| {
                focus.focus(window);
                state.update(cx, |state, _| {
                    state.start(event.position, event.click_count)
                });
                cx.notify(notify_owner);
            }
        })
        .on_mouse_move({
            let state = state.clone();
            move |event, _, cx| {
                if state.read(cx).is_selecting() {
                    state.update(cx, |state, _| state.update(event.position));
                    cx.notify(notify_owner);
                }
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let state = state.clone();
            move |_, _, cx| {
                state.update(cx, |state, _| state.finish());
                cx.notify(notify_owner);
            }
        })
        .on_action({
            let state = state.clone();
            move |_: &CopySelection, _, cx| copy(&state, cx)
        })
        .child(child)
        .context_menu(move |request, _, cx| {
            let current = copy_state.read(cx);
            if current.selected_text().is_none() || !current.selection_contains(request.position) {
                return ContextMenuSpec::default();
            }
            let state = copy_state.clone();
            ContextMenuSpec::new([ContextMenuItem::action(copy_label.clone(), move |_, cx| {
                copy(&state, cx)
            })])
        })
        .into_any_element()
}

fn copy(state: &Entity<SelectionState>, cx: &mut App) {
    if let Some(text) = state.read(cx).selected_text() {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}
