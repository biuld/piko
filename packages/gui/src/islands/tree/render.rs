//! Tree island rendering.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};

use crate::chrome::{IslandHeader, IslandPanel, IslandPlaceholder, IslandSessionPhase};
use crate::islands::agents::tree_guides;
use crate::theme::{RoleAccent, metrics, tokens};

use super::vm::{ConversationTreeViewModel, TreeEntryKind, TreeNode};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type IdClickFactory = Box<dyn Fn(String) -> ClickHandler>;

pub fn render_tree_panel(
    tree: &ConversationTreeViewModel,
    phase: IslandSessionPhase,
    focused: bool,
    on_tree_activate: IdClickFactory,
    on_tree_toggle_expand: IdClickFactory,
    on_switch_branch: ClickHandler,
) -> impl IntoElement {
    let m = metrics();
    let header = IslandHeader::title("Tree");
    let tree_panel = match phase {
        IslandSessionPhase::Idle => IslandPanel::empty(
            "conversation-tree",
            IslandPlaceholder::new("No session")
                .icon("▤")
                .subtitle("Select a session to see the tree"),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
        IslandSessionPhase::Loading => IslandPanel::loading(
            "conversation-tree",
            IslandPlaceholder::new("Loading tree…").icon("◌"),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
        IslandSessionPhase::Ready if tree.nodes.is_empty() => IslandPanel::empty(
            "conversation-tree",
            IslandPlaceholder::new("No conversation tree")
                .icon("▤")
                .subtitle("Tree entries appear as the agent works"),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
        IslandSessionPhase::Ready => IslandPanel::new(
            "conversation-tree",
            render_tree_nodes(tree, &on_tree_activate, &on_tree_toggle_expand),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
    };

    div()
        .id("conversation-tree-wrap")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(div().flex_1().min_h(px(0.)).min_w(px(0.)).child(tree_panel))
        .when(
            phase == IslandSessionPhase::Ready && tree.can_switch_branch,
            |d| {
                d.child(
                    div()
                        .p(m.space_sm)
                        .border_t_1()
                        .border_color(tokens().border_rgba())
                        .child(
                            Button::new("switch-branch")
                                .primary()
                                .label("Switch Branch")
                                .w_full()
                                .on_click(move |ev, w, cx| on_switch_branch(ev, w, cx)),
                        ),
                )
            },
        )
}

fn render_tree_nodes(
    tree: &ConversationTreeViewModel,
    on_tree_activate: &IdClickFactory,
    on_tree_toggle_expand: &IdClickFactory,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .children(tree.nodes.iter().enumerate().map(|(ix, node)| {
            let activate = on_tree_activate(node.id.clone());
            let toggle = on_tree_toggle_expand(node.id.clone());
            let previewed = tree.preview_entry_id.as_deref() == Some(node.id.as_str());
            render_tree_node(ix, node, previewed, activate, toggle)
        }))
}

fn render_tree_node(
    ix: usize,
    node: &TreeNode,
    previewed: bool,
    on_click: ClickHandler,
    on_toggle: ClickHandler,
) -> impl IntoElement {
    let m = metrics();
    let kind = match node.kind {
        TreeEntryKind::Message => "M",
        TreeEntryKind::Tool => "T",
        TreeEntryKind::System => "S",
        TreeEntryKind::Other => "·",
    };
    let accent = if node.is_leaf {
        tokens().role_accent(RoleAccent::Accent)
    } else if previewed {
        tokens().role_accent(RoleAccent::Warning)
    } else if node.on_path {
        tokens().fg_rgba()
    } else {
        tokens().muted_fg_rgba()
    };
    let chevron = if node.expanded { "▾" } else { "▸" };
    let toggle_id = SharedString::from(format!("tree-exp-{ix}"));

    div()
        .id(SharedString::from(format!("tree-node-{ix}")))
        .h(px(32.))
        .w_full()
        .px(m.space_sm)
        .flex()
        .items_center()
        .gap(m.space_xs)
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(tokens().elevated_rgba()))
        .when(node.is_leaf || previewed, |d| {
            d.bg(tokens().elevated_rgba())
        })
        .children(tree_guides(node.depth))
        .child(
            div()
                .w(px(16.))
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(if node.has_children {
                    div()
                        .id(toggle_id)
                        .text_size(m.meta_size)
                        .text_color(tokens().muted_fg_rgba())
                        .cursor_pointer()
                        .on_click(move |ev, w, cx| {
                            cx.stop_propagation();
                            on_toggle(ev, w, cx);
                        })
                        .child(chevron)
                        .into_any_element()
                } else {
                    div()
                        .text_size(m.meta_size)
                        .text_color(tokens().muted_fg_rgba())
                        .child("•")
                        .into_any_element()
                }),
        )
        .child(
            div()
                .w(px(16.))
                .flex_shrink_0()
                .text_center()
                .text_size(m.meta_size)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(accent)
                .child(kind),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(m.label_size)
                .line_height(m.label_line_height)
                .text_color(accent)
                .when(node.is_leaf || previewed, |d| {
                    d.font_weight(FontWeight::SEMIBOLD)
                })
                .child(node.label.clone()),
        )
        .when(node.on_path, |d| {
            d.child(
                div()
                    .flex_shrink_0()
                    .text_size(m.meta_size)
                    .text_color(tokens().muted_fg_rgba())
                    .child("path"),
            )
        })
        .on_click(move |ev, window, cx| on_click(ev, window, cx))
}
