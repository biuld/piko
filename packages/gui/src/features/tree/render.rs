//! Tree island rendering (display-only conversation map).

use gpui::*;

use crate::shell::{
    IslandHeader, IslandPanel, IslandPlaceholder, IslandSessionPhase, TreeClickHandler,
    TreeRowSpec, render_tree_list,
};
use crate::theme::{
    ChromeIcon, ChromeTokens, DomainRole, RoleAccent, domain_role_hsla, row_leading, tokens,
};

use super::vm::{ConversationTreeViewModel, TreeEntryKind, TreeNode};

type IdClickFactory = Box<dyn Fn(String) -> TreeClickHandler>;

pub fn render_tree_panel(
    tree: &ConversationTreeViewModel,
    phase: IslandSessionPhase,
    focused: bool,
    keyboard_index: Option<usize>,
    on_tree_activate: IdClickFactory,
    on_tree_toggle_expand: IdClickFactory,
) -> IslandPanel {
    let header = IslandHeader::title(crate::t!("island.tree.title"));
    match phase {
        IslandSessionPhase::Idle => IslandPanel::empty(
            "conversation-tree",
            IslandPlaceholder::new(crate::t!("island.tree.empty_no_session.title"))
                .chrome_icon(ChromeIcon::Network)
                .subtitle(crate::t!("island.tree.empty_no_session.subtitle")),
        )
        .header(header)
        .focused(focused),
        IslandSessionPhase::Loading => IslandPanel::loading(
            "conversation-tree",
            IslandPlaceholder::new(crate::t!("island.tree.loading"))
                .chrome_icon(ChromeIcon::CircleDashed),
        )
        .header(header)
        .focused(focused),
        IslandSessionPhase::Ready if tree.nodes.is_empty() => IslandPanel::empty(
            "conversation-tree",
            IslandPlaceholder::new(crate::t!("island.tree.empty.title"))
                .chrome_icon(ChromeIcon::Network)
                .subtitle(crate::t!("island.tree.empty.subtitle")),
        )
        .header(header)
        .focused(focused),
        IslandSessionPhase::Ready => IslandPanel::new(
            "conversation-tree",
            render_tree_nodes(
                tree,
                keyboard_index,
                &on_tree_activate,
                &on_tree_toggle_expand,
            ),
        )
        .header(header)
        .focused(focused),
    }
}

fn render_tree_nodes(
    tree: &ConversationTreeViewModel,
    keyboard_index: Option<usize>,
    on_tree_activate: &IdClickFactory,
    on_tree_toggle_expand: &IdClickFactory,
) -> impl IntoElement {
    let rows: Vec<_> = tree
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let previewed = tree.preview_entry_id.as_deref() == Some(node.id.as_str());
            let activate = on_tree_activate(node.id.clone());
            let toggle = on_tree_toggle_expand(node.id.clone());
            (
                conversation_row_spec(node, previewed, keyboard_index == Some(index)),
                activate,
                Some(toggle),
            )
        })
        .collect();
    render_tree_list(rows)
}

fn conversation_row_spec(node: &TreeNode, previewed: bool, keyboard_focused: bool) -> TreeRowSpec {
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
        keyboard_focused,
        show_guides: true,
        label: SharedString::from(node.label.clone()),
        label_color: Some(label_color),
        leading: Some(row_leading(kind_icon, kind_color)),
        detail: None,
        accessory: None,
        context_menu: None,
    }
}

fn tree_kind_icon(kind: TreeEntryKind) -> ChromeIcon {
    match kind {
        TreeEntryKind::User => ChromeIcon::User,
        TreeEntryKind::Assistant => ChromeIcon::Bot,
        TreeEntryKind::Tool => ChromeIcon::Wrench,
        TreeEntryKind::Model => ChromeIcon::Cpu,
        TreeEntryKind::Thinking => ChromeIcon::Brain,
        TreeEntryKind::Branch => ChromeIcon::GitBranch,
        TreeEntryKind::Compaction => ChromeIcon::Layers,
        TreeEntryKind::Other => ChromeIcon::Circle,
    }
}

fn tree_kind_color(kind: TreeEntryKind) -> gpui::Hsla {
    let t = tokens();
    match kind {
        TreeEntryKind::User => domain_role_hsla(DomainRole::User),
        TreeEntryKind::Assistant => domain_role_hsla(DomainRole::Assistant),
        TreeEntryKind::Tool => domain_role_hsla(DomainRole::Tool),
        TreeEntryKind::Thinking => domain_role_hsla(DomainRole::Thinking),
        TreeEntryKind::Model | TreeEntryKind::Branch | TreeEntryKind::Compaction => {
            domain_role_hsla(DomainRole::System)
        }
        TreeEntryKind::Other => ChromeTokens::hsla(t.muted_fg),
    }
}
