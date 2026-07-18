//! Tree island rendering (display-only conversation map).

use gpui::*;

use crate::chrome::{
    IslandHeader, IslandPanel, IslandPlaceholder, IslandSessionPhase, TreeClickHandler,
    TreeRowSpec, render_tree_list,
};
use crate::theme::{RoleAccent, metrics, tokens};

use super::vm::{ConversationTreeViewModel, TreeEntryKind, TreeNode};

type IdClickFactory = Box<dyn Fn(String) -> TreeClickHandler>;

pub fn render_tree_panel(
    tree: &ConversationTreeViewModel,
    phase: IslandSessionPhase,
    focused: bool,
    on_tree_activate: IdClickFactory,
    on_tree_toggle_expand: IdClickFactory,
) -> IslandPanel {
    let header = IslandHeader::title("Tree");
    match phase {
        IslandSessionPhase::Idle => IslandPanel::empty(
            "conversation-tree",
            IslandPlaceholder::new("No session")
                .icon("▤")
                .subtitle("Select a session to see the tree"),
        )
        .header(header)
        .focused(focused),
        IslandSessionPhase::Loading => IslandPanel::loading(
            "conversation-tree",
            IslandPlaceholder::new("Loading tree…").icon("◌"),
        )
        .header(header)
        .focused(focused),
        IslandSessionPhase::Ready if tree.nodes.is_empty() => IslandPanel::empty(
            "conversation-tree",
            IslandPlaceholder::new("No conversation tree")
                .icon("▤")
                .subtitle("Tree entries appear as the agent works"),
        )
        .header(header)
        .focused(focused),
        IslandSessionPhase::Ready => IslandPanel::new(
            "conversation-tree",
            render_tree_nodes(tree, &on_tree_activate, &on_tree_toggle_expand),
        )
        .header(header)
        .focused(focused),
    }
}

fn render_tree_nodes(
    tree: &ConversationTreeViewModel,
    on_tree_activate: &IdClickFactory,
    on_tree_toggle_expand: &IdClickFactory,
) -> impl IntoElement {
    let meta_size = metrics().meta_size;
    let rows: Vec<_> = tree
        .nodes
        .iter()
        .map(|node| {
            let previewed = tree.preview_entry_id.as_deref() == Some(node.id.as_str());
            let activate = on_tree_activate(node.id.clone());
            let toggle = on_tree_toggle_expand(node.id.clone());
            (
                conversation_row_spec(node, previewed, meta_size),
                activate,
                Some(toggle),
            )
        })
        .collect();
    render_tree_list(rows)
}

fn conversation_row_spec(node: &TreeNode, previewed: bool, meta_size: Pixels) -> TreeRowSpec {
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

    let leading = div()
        .w(px(16.))
        .flex_shrink_0()
        .text_center()
        .text_size(meta_size)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(accent)
        .child(kind)
        .into_any_element();

    let trailing = if node.on_path {
        Some(
            div()
                .flex_shrink_0()
                .text_size(meta_size)
                .text_color(tokens().muted_fg_rgba())
                .child("path")
                .into_any_element(),
        )
    } else {
        None
    };

    TreeRowSpec {
        id: SharedString::from(node.id.clone()),
        depth: node.depth,
        has_children: node.has_children,
        expanded: node.expanded,
        selected: false,
        emphasized: node.is_leaf || previewed,
        show_guides: true,
        label: SharedString::from(node.label.clone()),
        label_color: Some(accent),
        leading: Some(leading),
        trailing,
    }
}
