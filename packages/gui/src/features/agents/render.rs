//! Compact file-tree-style rendering for Agent instances.

use std::collections::HashSet;

use gpui::*;

use crate::shell::{
    IslandHeader, IslandPanel, IslandPlaceholder, IslandSessionPhase, TreeClickHandler,
    TreeRowSpec, render_tree_list,
};
use crate::theme::{ChromeIcon, ChromeTokens, row_leading, tokens};

use super::vm::{AgentTreeNode, AgentTreeViewModel, agent_node_visible};

type ClickHandler = TreeClickHandler;
type IdClickFactory = Box<dyn Fn(String) -> ClickHandler>;

pub fn render_agent_tree_panel(
    vm: &AgentTreeViewModel,
    collapsed: &HashSet<String>,
    phase: IslandSessionPhase,
    focused: bool,
    keyboard_id: Option<&str>,
    on_select: IdClickFactory,
    on_toggle: IdClickFactory,
) -> impl IntoElement {
    let header = IslandHeader::title(crate::t!("island.agents.title"));
    match phase {
        IslandSessionPhase::Idle => IslandPanel::empty(
            "agent-tree",
            IslandPlaceholder::new(crate::t!("island.agents.empty_no_session.title"))
                .chrome_icon(ChromeIcon::Bot)
                .subtitle(crate::t!("island.agents.empty_no_session.subtitle")),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
        IslandSessionPhase::Loading => IslandPanel::loading(
            "agent-tree",
            IslandPlaceholder::new(crate::t!("island.agents.loading"))
                .chrome_icon(ChromeIcon::CircleDashed),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
        IslandSessionPhase::Ready if vm.nodes.is_empty() => IslandPanel::empty(
            "agent-tree",
            IslandPlaceholder::new(crate::t!("island.agents.empty.title"))
                .chrome_icon(ChromeIcon::Bot)
                .subtitle(crate::t!("island.agents.empty.subtitle")),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
        IslandSessionPhase::Ready => IslandPanel::new(
            "agent-tree",
            render_agent_tree_body(vm, collapsed, keyboard_id, &on_select, &on_toggle),
        )
        .header(header)
        .focused(focused)
        .into_any_element(),
    }
}

/// Visible agent rows (same filter as the panel body) for keyboard indexing.
pub fn visible_agent_ids(vm: &AgentTreeViewModel, collapsed: &HashSet<String>) -> Vec<String> {
    vm.nodes
        .iter()
        .filter(|node| agent_node_visible(node, &vm.nodes, collapsed))
        .map(|n| n.agent_instance_id.clone())
        .collect()
}

fn render_agent_tree_body(
    vm: &AgentTreeViewModel,
    collapsed: &HashSet<String>,
    keyboard_id: Option<&str>,
    on_select: &IdClickFactory,
    on_toggle: &IdClickFactory,
) -> impl IntoElement {
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
                agent_row_spec(node, collapsed, keyboard_id),
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
    keyboard_id: Option<&str>,
) -> TreeRowSpec {
    let t = tokens();
    let leading_color = if node.selected {
        ChromeTokens::hsla(t.accent)
    } else {
        ChromeTokens::hsla(t.muted_fg)
    };
    let trailing = div()
        .flex_shrink_0()
        .child(
            crate::theme::text(crate::theme::TextRole::Meta)
                .text_color(t.muted_fg_rgba())
                .child(format!("{} · {}", node.role, node.activity_label)),
        )
        .into_any_element();

    TreeRowSpec {
        id: SharedString::from(node.agent_instance_id.clone()),
        depth: node.depth,
        has_children: node.has_children,
        expanded: !collapsed.contains(&node.agent_instance_id),
        selected: node.selected,
        emphasized: false,
        keyboard_focused: keyboard_id == Some(node.agent_instance_id.as_str()),
        show_guides: true,
        label: SharedString::from(node.name.clone()),
        label_color: None,
        leading: Some(row_leading(ChromeIcon::Bot, leading_color)),
        detail: Some(trailing),
        accessory: None,
        context_menu: None,
    }
}
