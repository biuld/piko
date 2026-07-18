//! Compact file-tree-style rendering for Agent instances.

use std::collections::HashSet;

use gpui::*;

use crate::chrome::{
    IslandHeader, IslandPanel, IslandPlaceholder, IslandSessionPhase, TreeClickHandler,
    TreeRowSpec, render_tree_list,
};
use crate::theme::metrics;

use super::vm::{AgentTreeNode, AgentTreeViewModel, agent_node_visible};

type ClickHandler = TreeClickHandler;
type IdClickFactory = Box<dyn Fn(String) -> ClickHandler>;

pub fn render_agent_tree_panel(
    vm: &AgentTreeViewModel,
    collapsed: &HashSet<String>,
    phase: IslandSessionPhase,
    focused: bool,
    on_select: IdClickFactory,
    on_toggle: IdClickFactory,
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
        IslandSessionPhase::Ready => IslandPanel::new(
            "agent-tree",
            render_agent_tree_body(vm, collapsed, &on_select, &on_toggle),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
    }
}

fn render_agent_tree_body(
    vm: &AgentTreeViewModel,
    collapsed: &HashSet<String>,
    on_select: &IdClickFactory,
    on_toggle: &IdClickFactory,
) -> impl IntoElement {
    let meta_size = metrics().meta_size;
    let meta_line = metrics().meta_line_height;
    let rows: Vec<_> = vm
        .nodes
        .iter()
        .filter(|node| agent_node_visible(node, &vm.nodes, collapsed))
        .map(|node| {
            let id = node.agent_instance_id.clone();
            let activate = on_select(id.clone());
            let toggle = if node.has_children {
                Some(on_toggle(id))
            } else {
                None
            };
            (
                agent_row_spec(node, collapsed, meta_size, meta_line),
                activate,
                toggle,
            )
        })
        .collect();
    render_tree_list(rows)
}

fn agent_row_spec(
    node: &AgentTreeNode,
    collapsed: &HashSet<String>,
    meta_size: Pixels,
    meta_line: Pixels,
) -> TreeRowSpec {
    let trailing = div()
        .flex_shrink_0()
        .text_size(meta_size)
        .line_height(meta_line)
        .text_color(crate::theme::tokens().muted_fg_rgba())
        .child(format!("{} · {}", node.role, node.activity_label))
        .into_any_element();

    TreeRowSpec {
        id: SharedString::from(node.agent_instance_id.clone()),
        depth: node.depth,
        has_children: node.has_children,
        expanded: !collapsed.contains(&node.agent_instance_id),
        selected: node.selected,
        emphasized: false,
        show_guides: true,
        label: SharedString::from(node.name.clone()),
        label_color: None,
        leading: None,
        trailing: Some(trailing),
    }
}
