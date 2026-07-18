//! Compact file-tree-style rendering for Agent instances.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::scroll::ScrollableElement;

use crate::theme::{RoleAccent, island, metrics, tokens};

use super::{AgentTreeNode, AgentTreeViewModel};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn render_agent_tree_panel(
    vm: &AgentTreeViewModel,
    on_select: impl Fn(String) -> ClickHandler,
) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    island()
        .id("agent-tree")
        .size_full()
        .flex()
        .flex_col()
        .bg(t.surface_rgba())
        .child(
            div()
                .h(m.panel_header_height)
                .px(m.space_md)
                .flex()
                .items_center()
                .text_size(m.label_size)
                .line_height(m.label_line_height)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Agents"),
        )
        .child(
            div()
                .flex_1()
                .overflow_y_scrollbar()
                .p(m.space_sm)
                .children(vm.nodes.iter().enumerate().map(|(ix, node)| {
                    let handler = on_select(node.agent_instance_id.clone());
                    render_agent_node(ix, node, handler)
                })),
        )
}

fn render_agent_node(ix: usize, node: &AgentTreeNode, on_click: ClickHandler) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    let marker = if node.has_children { "▾" } else { "•" };
    div()
        .id(SharedString::from(format!("agent-tree-{ix}")))
        .h(px(32.))
        .w_full()
        .px(m.space_sm)
        .flex()
        .items_center()
        .gap(m.space_xs)
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(t.elevated_rgba()))
        .when(node.selected, |d| d.bg(t.elevated_rgba()))
        .children(tree_guides(node.depth))
        .child(
            div()
                .w(px(16.))
                .flex_shrink_0()
                .text_center()
                .text_size(m.meta_size)
                .text_color(if node.selected {
                    t.role_accent(RoleAccent::Accent)
                } else {
                    t.muted_fg_rgba()
                })
                .child(marker),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(m.label_size)
                .line_height(m.label_line_height)
                .when(node.selected, |d| {
                    d.font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.role_accent(RoleAccent::Accent))
                })
                .child(node.name.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(m.meta_size)
                .line_height(m.meta_line_height)
                .text_color(t.muted_fg_rgba())
                .child(format!("{} · {}", node.role, node.activity_label)),
        )
        .on_click(move |ev, window, cx| on_click(ev, window, cx))
}

pub(crate) fn tree_guides(depth: usize) -> Vec<AnyElement> {
    let t = tokens();
    (0..depth)
        .map(|_| {
            div()
                .w(px(16.))
                .h_full()
                .flex_shrink_0()
                .border_l_1()
                .border_color(t.border_rgba())
                .into_any_element()
        })
        .collect()
}
