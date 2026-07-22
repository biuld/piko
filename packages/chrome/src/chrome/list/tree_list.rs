//! Shared tree row primitives for island lists (TreeList composite).
//!
//! ## Composite contract (roadmap D4)
//!
//! 1. App flattens the domain tree (expansion filter stays app-owned).
//! 2. App holds [`super::ListKeyboard`] over visible row count when the island
//!    has a keyboard cursor.
//! 3. App builds [`TreeRowSpec`] with `keyboard_focused` from the cursor
//!    (independent of `selected` / domain selection).
//! 4. App paints via [`render_tree_list`] / [`render_tree_row`] only — no
//!    parallel tree row chrome.
//!
//! Islands own expansion filtering and domain view-models; this module only
//! renders flat visible rows with depth guides and a separate disclosure hit
//! target (depth guides and tool-window row rails).

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::menu::ContextMenuExt;
use gpui_component::menu::PopupMenu;
use std::rc::Rc;

use crate::theme::{metrics, tokens};

pub type TreeClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub type TreeContextMenuBuilder =
    Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// Mutually exclusive content for the fixed trailing accessory rail.
pub enum TreeRowAccessory {
    Meta(SharedString),
    Action(AnyElement),
}

/// One visible flattened tree row. Islands filter collapsed children before
/// building the list.
pub struct TreeRowSpec {
    pub id: SharedString,
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
    /// Elevated background (selection / live target).
    pub selected: bool,
    /// Elevated background + semibold label (leaf / preview).
    pub emphasized: bool,
    /// Keyboard caret on this row (focus-visible ring); independent of selection.
    pub keyboard_focused: bool,
    /// Draw vertical depth-guide lines (indent slots remain either way).
    pub show_guides: bool,
    pub label: SharedString,
    /// Overrides default / selected label color when set.
    pub label_color: Option<Rgba>,
    pub leading: Option<AnyElement>,
    /// Optional intrinsic-width context before the fixed trailing rails.
    pub detail: Option<AnyElement>,
    /// Centered read-only metadata or action in the fixed accessory rail.
    pub accessory: Option<TreeRowAccessory>,
    /// Right-click menu for session rows and similar.
    pub context_menu: Option<TreeContextMenuBuilder>,
}

/// Depth indent slots: one 16 px column per ancestor level.
/// When `show_lines` is true, each slot draws a left border guide.
pub fn tree_guides(depth: usize, show_lines: bool) -> Vec<AnyElement> {
    let t = tokens();
    (0..depth)
        .map(|_| {
            div()
                .w(px(16.))
                .h_full()
                .flex_shrink_0()
                .when(show_lines, |d| d.border_l_1().border_color(t.border_rgba()))
                .into_any_element()
        })
        .collect()
}

/// Render a vertical stack of tree rows.
pub fn render_tree_list(
    rows: impl IntoIterator<Item = (TreeRowSpec, TreeClickHandler, Option<TreeClickHandler>)>,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .children(
            rows.into_iter()
                .enumerate()
                .map(|(ix, (spec, on_activate, on_toggle))| {
                    render_tree_row(ix, spec, on_activate, on_toggle)
                }),
        )
}

pub fn render_tree_row(
    ix: usize,
    spec: TreeRowSpec,
    on_activate: TreeClickHandler,
    on_toggle: Option<TreeClickHandler>,
) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let mute = crate::theme::ChromeTokens::hsla(t.muted_fg);
    let toggle_id = SharedString::from(format!("tree-row-exp-{ix}"));
    let row_id = SharedString::from(format!("tree-row-{}-{ix}", spec.id));
    let chrome = tree_row_chrome(&spec);
    let label_color = spec.label_color.unwrap_or_else(|| {
        if spec.selected {
            t.accent_rgba()
        } else {
            t.fg_rgba()
        }
    });

    let row = div()
        .id(row_id)
        .h(px(32.))
        .w_full()
        .px(m.tool_row_inset)
        .flex()
        .items_center()
        .gap(m.space_xs)
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(t.elevated_rgba()))
        .when(chrome.elevated_bg, |d| d.bg(t.elevated_rgba()))
        .when(chrome.focus_ring, |d| {
            d.border_1().border_color(t.ring_rgba())
        })
        .children(tree_guides(spec.depth, spec.show_guides))
        .children(spec.leading)
        .child(
            crate::theme::label_text(chrome.semibold_label)
                .min_w_0()
                .flex_1()
                .truncate()
                .text_color(label_color)
                .child(spec.label),
        )
        .children(spec.detail)
        // Disclosure describes tree structure, so it precedes the terminal
        // accessory rail. The rail is reserved even for leaf rows.
        .child(
            div()
                .id(toggle_id)
                .w(m.tool_disclosure_width)
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .when(spec.has_children, |d| {
                    let toggle = on_toggle.unwrap_or_else(|| {
                        Box::new(|_: &ClickEvent, _: &mut Window, _: &mut App| {})
                    });
                    d.cursor_pointer()
                        .on_click(move |ev, w, cx| {
                            cx.stop_propagation();
                            toggle(ev, w, cx);
                        })
                        .child(crate::theme::disclosure(spec.expanded, mute))
                }),
        )
        // Always reserve one stable accessory rail. Its content semantics do
        // not affect its terminal right-edge position.
        .child(
            div()
                .w(m.tool_accessory_width)
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .children(spec.accessory.map(|accessory| {
                    match accessory {
                        TreeRowAccessory::Meta(value) => {
                            crate::theme::text(crate::theme::TextRole::Meta)
                                .text_color(t.muted_fg_rgba())
                                .child(value)
                                .into_any_element()
                        }
                        TreeRowAccessory::Action(action) => action,
                    }
                })),
        )
        .on_click(move |ev, window, cx| on_activate(ev, window, cx));

    if let Some(build_menu) = spec.context_menu {
        row.context_menu(move |menu, window, cx| build_menu(menu, window, cx))
            .into_any_element()
    } else {
        row.into_any_element()
    }
}

/// Pure paint flags for a tree row (unit-testable without a window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeRowChrome {
    pub elevated_bg: bool,
    pub focus_ring: bool,
    pub semibold_label: bool,
}

/// Resolve chrome paint flags from a tree row spec.
pub fn tree_row_chrome(spec: &TreeRowSpec) -> TreeRowChrome {
    TreeRowChrome {
        elevated_bg: spec.selected || spec.emphasized,
        focus_ring: spec.keyboard_focused,
        semibold_label: spec.selected || spec.emphasized,
    }
}

#[cfg(test)]
mod tests {
    // Avoid `use super::*` — pulls GPUI into #[test] expansion (recursion limit).
    use super::{TreeRowSpec, tree_row_chrome};

    fn bare_row(selected: bool, keyboard_focused: bool, emphasized: bool) -> TreeRowSpec {
        TreeRowSpec {
            id: "n1".into(),
            depth: 1,
            has_children: false,
            expanded: false,
            selected,
            emphasized,
            keyboard_focused,
            show_guides: true,
            label: "Node".into(),
            label_color: None,
            leading: None,
            detail: None,
            accessory: None,
            context_menu: None,
        }
    }

    #[test]
    fn keyboard_focused_is_independent_of_selected() {
        let selected_only = bare_row(true, false, false);
        let c = tree_row_chrome(&selected_only);
        assert!(c.elevated_bg);
        assert!(!c.focus_ring);

        let kb_only = bare_row(false, true, false);
        let c = tree_row_chrome(&kb_only);
        assert!(!c.elevated_bg);
        assert!(c.focus_ring);

        let both = bare_row(true, true, false);
        let c = tree_row_chrome(&both);
        assert!(c.elevated_bg && c.focus_ring);
    }

    #[test]
    fn emphasized_raises_background_without_focus_ring() {
        let c = tree_row_chrome(&bare_row(false, false, true));
        assert!(c.elevated_bg);
        assert!(c.semibold_label);
        assert!(!c.focus_ring);
    }
}
