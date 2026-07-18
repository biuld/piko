//! Session sidebar panel body for [`super::SessionsIsland`].

use std::collections::HashSet;

use gpui::*;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};

use crate::chrome::{
    IslandHeader, IslandPanel, IslandPlaceholder, TreeClickHandler, TreeRowSpec, render_tree_list,
};
use crate::projections::{SessionRow, SessionRowKind, SidebarGroup, SidebarViewModel};
use crate::theme::{IconSize, PikoIcon, PikoTokens, icon, row_leading, tokens};

pub(crate) type ClickHandler = TreeClickHandler;
pub(crate) type IdClickFactory = Box<dyn Fn(String) -> ClickHandler>;

pub(crate) fn render_sidebar_panel(
    vm: &SidebarViewModel,
    collapsed: &HashSet<String>,
    on_new_session: ClickHandler,
    on_open_session: IdClickFactory,
    on_toggle_dir: IdClickFactory,
    focused: bool,
) -> IslandPanel {
    let new_session = Button::new("new-session")
        .icon(icon(
            PikoIcon::Plus,
            IconSize::Label,
            PikoTokens::hsla(tokens().muted_fg),
        ))
        .tooltip(crate::t!("island.sessions.action.new"))
        .ghost()
        .small()
        .compact()
        .px(px(0.))
        .on_click(move |ev, window, cx| on_new_session(ev, window, cx))
        .into_any_element();

    let has_rows = vm.groups.iter().any(|g| !g.rows.is_empty());
    let header =
        IslandHeader::title_with_actions(crate::t!("island.sessions.title"), [new_session]);

    if !has_rows {
        return IslandPanel::empty(
            "sessions-island",
            IslandPlaceholder::new(crate::t!("island.sessions.empty.title"))
                .piko_icon(PikoIcon::Circle)
                .subtitle(crate::t!("island.sessions.empty.subtitle")),
        )
        .header(header)
        .focused(focused);
    }

    let body = render_tree_list(flatten_session_rows(
        vm,
        collapsed,
        &on_open_session,
        &on_toggle_dir,
    ));

    IslandPanel::new("sessions-island", body)
        .header(header)
        .focused(focused)
}

fn group_key(group: &SidebarGroup) -> String {
    if group.cwd.is_empty() {
        String::new()
    } else {
        group.cwd.clone()
    }
}

fn flatten_session_rows(
    vm: &SidebarViewModel,
    collapsed: &HashSet<String>,
    on_open: &IdClickFactory,
    on_toggle: &IdClickFactory,
) -> Vec<(TreeRowSpec, ClickHandler, Option<ClickHandler>)> {
    let t = tokens();
    let mut out = Vec::new();

    for group in &vm.groups {
        if group.rows.is_empty() {
            continue;
        }
        let key = group_key(group);
        let expanded = !collapsed.contains(&key);
        let dir_id = if key.is_empty() {
            "session-dir-pending".to_string()
        } else {
            format!("session-dir-{key}")
        };

        let toggle = on_toggle(key.clone());
        let activate = on_toggle(key.clone());

        let mute = PikoTokens::hsla(t.muted_fg);
        let dir_icon = if expanded {
            PikoIcon::FolderOpen
        } else {
            PikoIcon::Folder
        };

        out.push((
            TreeRowSpec {
                id: SharedString::from(dir_id),
                depth: 0,
                has_children: true,
                expanded,
                selected: false,
                emphasized: false,
                show_guides: false,
                label: SharedString::from(group.label.clone()),
                label_color: Some(t.muted_fg_rgba()),
                leading: Some(row_leading(dir_icon, mute)),
                trailing: None,
            },
            activate,
            Some(toggle),
        ));

        if !expanded {
            continue;
        }

        for row in &group.rows {
            out.push(session_tree_row(row, on_open));
        }
    }

    out
}

fn session_tree_row(
    row: &SessionRow,
    on_open: &IdClickFactory,
) -> (TreeRowSpec, ClickHandler, Option<ClickHandler>) {
    let t = tokens();
    let is_live = row.kind == SessionRowKind::LiveTarget;
    let is_pending = row.kind == SessionRowKind::PendingTarget;

    let label_color = if is_pending {
        Some(t.muted_fg_rgba())
    } else if is_live {
        None // selected drives accent
    } else {
        Some(t.fg_rgba())
    };

    let leading_color = if is_pending {
        PikoTokens::hsla(t.muted_fg)
    } else if is_live {
        PikoTokens::hsla(t.accent)
    } else {
        PikoTokens::hsla(t.fg)
    };

    let trailing = if row.message_count > 0 {
        Some(
            crate::theme::text(crate::theme::TextRole::Meta)
                .flex_shrink_0()
                .text_color(t.muted_fg_rgba())
                .child(row.message_count.to_string())
                .into_any_element(),
        )
    } else {
        None
    };

    (
        TreeRowSpec {
            id: SharedString::from(format!("session-{}", row.session_id)),
            depth: 1,
            has_children: false,
            expanded: false,
            selected: is_live,
            emphasized: is_pending,
            show_guides: false,
            label: SharedString::from(row.label.clone()),
            label_color,
            leading: Some(row_leading(PikoIcon::MessageSquare, leading_color)),
            trailing,
        },
        on_open(row.session_id.clone()),
        None,
    )
}
