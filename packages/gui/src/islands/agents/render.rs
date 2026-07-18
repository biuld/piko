//! Compact file-tree-style rendering for Agent instances.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::chrome::{IslandHeader, IslandPanel, IslandPlaceholder, IslandSessionPhase};
use crate::theme::{RoleAccent, metrics, tokens};

use super::vm::{AgentTreeNode, AgentTreeViewModel};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn render_agent_tree_panel(
    vm: &AgentTreeViewModel,
    phase: IslandSessionPhase,
    focused: bool,
    on_select: impl Fn(String) -> ClickHandler,
) -> impl IntoElement {
    let header = IslandHeader::title("Agents");
    match phase {
        IslandSessionPhase::Idle => IslandPanel::empty(
            "agent-tree",
            IslandPlaceholder::new("No session")
                .icon("◇")
                .subtitle("Select a session to see agents"),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
        IslandSessionPhase::Loading => IslandPanel::loading(
            "agent-tree",
            IslandPlaceholder::new("Loading agents…").icon("◌"),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
        IslandSessionPhase::Ready if vm.nodes.is_empty() => IslandPanel::empty(
            "agent-tree",
            IslandPlaceholder::new("No agents")
                .icon("◇")
                .subtitle("Agents appear as the session runs"),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
        IslandSessionPhase::Ready => {
            IslandPanel::new("agent-tree", render_agent_tree_body(vm, on_select))
                .header(header)
                .focused(focused)
                .into_any_element()
        }
    }
}

pub fn render_agent_tree_body(
    vm: &AgentTreeViewModel,
    on_select: impl Fn(String) -> ClickHandler,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .children(vm.nodes.iter().enumerate().map(|(ix, node)| {
            let handler = on_select(node.agent_instance_id.clone());
            render_agent_node(ix, node, handler)
        }))
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

pub fn tree_guides(depth: usize) -> Vec<AnyElement> {
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
