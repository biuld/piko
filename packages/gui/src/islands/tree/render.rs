//! Tree island rendering (display-only conversation map).

use gpui::*;

use crate::chrome::{
    IslandHeader, IslandPanel, IslandPlaceholder, IslandSessionPhase, TreeClickHandler,
    TreeRowSpec, render_tree_list,
};
use crate::theme::{PikoIcon, PikoTokens, RoleAccent, row_leading, tokens};

use super::vm::{ConversationTreeViewModel, TreeEntryKind, TreeNode};

type IdClickFactory = Box<dyn Fn(String) -> TreeClickHandler>;

pub fn render_tree_panel(
    tree: &ConversationTreeViewModel,
    phase: IslandSessionPhase,
    focused: bool,
    on_tree_activate: IdClickFactory,
    on_tree_toggle_expand: IdClickFactory,
) -> IslandPanel {
    let header = IslandHeader::title(crate::t!("island.tree.title"));
    match phase {
        IslandSessionPhase::Idle => IslandPanel::empty(
            "conversation-tree",
            IslandPlaceholder::new(crate::t!("island.tree.empty_no_session.title"))
                .piko_icon(PikoIcon::Network)
                .subtitle(crate::t!("island.tree.empty_no_session.subtitle")),
        )
        .header(header)
        .focused(focused),
        IslandSessionPhase::Loading => IslandPanel::loading(
            "conversation-tree",
            IslandPlaceholder::new(crate::t!("island.tree.loading"))
                .piko_icon(PikoIcon::CircleDashed),
        )
        .header(header)
        .focused(focused),
        IslandSessionPhase::Ready if tree.nodes.is_empty() => IslandPanel::empty(
            "conversation-tree",
            IslandPlaceholder::new(crate::t!("island.tree.empty.title"))
                .piko_icon(PikoIcon::Network)
                .subtitle(crate::t!("island.tree.empty.subtitle")),
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
    let rows: Vec<_> = tree
        .nodes
        .iter()
        .map(|node| {
            let previewed = tree.preview_entry_id.as_deref() == Some(node.id.as_str());
            let activate = on_tree_activate(node.id.clone());
            let toggle = on_tree_toggle_expand(node.id.clone());
            (
                conversation_row_spec(node, previewed),
                activate,
                Some(toggle),
            )
        })
        .collect();
    render_tree_list(rows)
}

fn conversation_row_spec(node: &TreeNode, previewed: bool) -> TreeRowSpec {
    let t = tokens();
    let kind_icon = tree_kind_icon(node.kind);
    let kind_color = tree_kind_color(node.kind);

    // Active path / leaf / preview via label color + weight; kind owns icon tint.
    let label_color = if node.is_leaf {
        t.role_accent(RoleAccent::Accent)
    } else if previewed {
        t.role_accent(RoleAccent::Warning)
    } else if node.on_path {
        t.fg_rgba()
    } else {
        t.muted_fg_rgba()
    };

    TreeRowSpec {
        id: SharedString::from(node.id.clone()),
        depth: node.depth,
        has_children: node.has_children,
        expanded: node.expanded,
        selected: false,
        emphasized: node.is_leaf || previewed || node.on_path,
        show_guides: true,
        label: SharedString::from(node.label.clone()),
        label_color: Some(label_color),
        leading: Some(row_leading(kind_icon, kind_color)),
        trailing: None,
    }
}

fn tree_kind_icon(kind: TreeEntryKind) -> PikoIcon {
    match kind {
        TreeEntryKind::User => PikoIcon::User,
        TreeEntryKind::Assistant => PikoIcon::Bot,
        TreeEntryKind::Tool => PikoIcon::Wrench,
        TreeEntryKind::Model => PikoIcon::Cpu,
        TreeEntryKind::Thinking => PikoIcon::Brain,
        TreeEntryKind::Branch => PikoIcon::GitBranch,
        TreeEntryKind::Compaction => PikoIcon::Layers,
        TreeEntryKind::Other => PikoIcon::Circle,
    }
}

fn tree_kind_color(kind: TreeEntryKind) -> gpui::Hsla {
    let t = tokens();
    match kind {
        TreeEntryKind::User => t.role_accent_hsla(RoleAccent::User),
        TreeEntryKind::Assistant => t.role_accent_hsla(RoleAccent::Assistant),
        TreeEntryKind::Tool => t.role_accent_hsla(RoleAccent::Tool),
        TreeEntryKind::Thinking => t.role_accent_hsla(RoleAccent::Thinking),
        TreeEntryKind::Model | TreeEntryKind::Branch | TreeEntryKind::Compaction => {
            t.role_accent_hsla(RoleAccent::System)
        }
        TreeEntryKind::Other => PikoTokens::hsla(t.muted_fg),
    }
}
