//! Shared Workbench tree row primitives (Sessions, Tree, Agents).
//!
//! Islands own expansion filtering and domain view-models; this module only
//! renders flat visible rows with depth guides and a separate disclosure hit
//! target (see ui-guidelines Trees).

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::theme::{metrics, tokens};

pub type TreeClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

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
    /// Draw vertical depth-guide lines (indent slots remain either way).
    pub show_guides: bool,
    pub label: SharedString,
    /// Overrides default / selected label color when set.
    pub label_color: Option<Rgba>,
    pub leading: Option<AnyElement>,
    pub trailing: Option<AnyElement>,
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
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();
    let mute = crate::theme::PikoTokens::hsla(t.muted_fg);
    let toggle_id = SharedString::from(format!("tree-row-exp-{ix}"));
    let row_id = SharedString::from(format!("tree-row-{}-{ix}", spec.id));
    let fill = spec.selected || spec.emphasized;
    let label_color = spec.label_color.unwrap_or_else(|| {
        if spec.selected {
            t.role_accent(crate::theme::RoleAccent::Accent)
        } else {
            t.fg_rgba()
        }
    });
    let semibold = spec.selected || spec.emphasized;

    div()
        .id(row_id)
        .h(px(32.))
        .w_full()
        .px(m.space_sm)
        .flex()
        .items_center()
        .gap(m.space_xs)
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(t.elevated_rgba()))
        .when(fill, |d| d.bg(t.elevated_rgba()))
        .children(tree_guides(spec.depth, spec.show_guides))
        .children(spec.leading)
        .child(
            crate::theme::label_text(semibold)
                .min_w_0()
                .flex_1()
                .truncate()
                .text_color(label_color)
                .child(spec.label),
        )
        .children(spec.trailing)
        // Fixed disclosure column on the right (ui-guidelines): always 16 px;
        // chevron only when the row is expandable (`has_children`).
        .child(
            div()
                .id(toggle_id)
                .w(px(16.))
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
        .on_click(move |ev, window, cx| on_activate(ev, window, cx))
}
