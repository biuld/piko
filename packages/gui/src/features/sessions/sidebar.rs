//! Session sidebar panel body for [`super::SessionsIsland`].

use std::collections::HashSet;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use piko_chrome::components::menu::{ContextMenuItem, ContextMenuItemTone, ContextMenuSpec};

use crate::projections::{SessionRow, SessionRowKind, SidebarGroup, SidebarViewModel};
use crate::shell::{
    IslandContentViewport, IslandHeader, IslandPanel, IslandPlaceholder, TreeClickHandler,
    TreeContextMenuBuilder, TreeRowAccessory, TreeRowSpec, render_tree_list,
};
use crate::theme::{
    ChromeIcon, ChromeTokens, IconSize, TextRole, icon, metrics, row_leading, text, tokens,
};

pub(crate) type ClickHandler = TreeClickHandler;
pub(crate) type IdClickFactory = Box<dyn Fn(String) -> ClickHandler>;
pub(crate) type SessionMenuAction = Rc<dyn Fn(&mut Window, &mut App)>;
pub(crate) type SessionMenuFactory = Box<dyn Fn(&SessionRow) -> Option<TreeContextMenuBuilder>>;
pub(crate) type SearchFocusHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionListTarget {
    Directory(String),
    Session(String),
}

pub(crate) struct SidebarPanelHandlers<'a> {
    pub on_open_session: &'a IdClickFactory,
    pub on_new_session: &'a IdClickFactory,
    pub on_toggle_dir: &'a IdClickFactory,
    pub session_menu: &'a SessionMenuFactory,
    pub on_search_focus: &'a SearchFocusHandler,
}

/// `has_sessions` is true when the host list has rows before search filter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_sidebar_panel(
    vm: &SidebarViewModel,
    has_sessions: bool,
    collapsed: &HashSet<String>,
    search_input: Entity<InputState>,
    list_scroll: ScrollHandle,
    on_open_directory: ClickHandler,
    handlers: SidebarPanelHandlers<'_>,
    focused: bool,
    keyboard_index: Option<usize>,
) -> IslandPanel {
    let open_directory = Button::new("open-directory")
        .icon(icon(
            ChromeIcon::FolderOpen,
            IconSize::Label,
            ChromeTokens::hsla(tokens().muted_fg),
        ))
        .tooltip(crate::t!("island.sessions.action.open_directory"))
        .ghost()
        .small()
        .compact()
        .px(px(0.))
        .on_click(move |ev, window, cx| on_open_directory(ev, window, cx))
        .into_any_element();

    let header =
        IslandHeader::title_with_action(crate::t!("island.sessions.title"), open_directory);

    if !has_sessions {
        return IslandPanel::empty(
            "sessions-island",
            IslandPlaceholder::new(crate::t!("island.sessions.empty.title"))
                .chrome_icon(ChromeIcon::Circle)
                .subtitle(crate::t!("island.sessions.empty.subtitle")),
        )
        .header(header)
        .focused(focused);
    }

    let mut tree_rows = flatten_session_rows(
        vm,
        collapsed,
        handlers.on_open_session,
        handlers.on_new_session,
        handlers.on_toggle_dir,
        handlers.session_menu,
    );
    for (index, (spec, _, _)) in tree_rows.iter_mut().enumerate() {
        spec.keyboard_focused = keyboard_index == Some(index);
    }

    let m = metrics();
    let on_search_focus = handlers.on_search_focus.clone();
    let list_body = if tree_rows.is_empty() {
        div()
            .w_full()
            .px(m.tool_row_inset)
            .py(m.space_lg)
            .child(
                text(TextRole::Meta)
                    .text_color(tokens().muted_fg_rgba())
                    .child(crate::t!("island.sessions.search.no_matches")),
            )
            .into_any_element()
    } else {
        render_tree_list(tree_rows).into_any_element()
    };
    let list = IslandContentViewport::new("sessions-island-viewport", list_scroll, list_body);

    let body = div()
        .flex()
        .flex_col()
        .size_full()
        .min_h(px(0.))
        .child(
            div()
                .id("sessions-search")
                .key_context("IslandSessionsSearch")
                .px(m.tool_row_inset)
                .pb(m.space_sm)
                .flex_shrink_0()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    on_search_focus(window, cx);
                })
                .child(Input::new(&search_input).w_full()),
        )
        .when(!vm.pinned.is_empty(), |d| {
            d.child(
                div()
                    .px(m.tool_row_inset)
                    .pb(m.space_xs)
                    .flex_shrink_0()
                    .child(
                        text(TextRole::Meta)
                            .text_color(tokens().muted_fg_rgba())
                            .child(crate::t!("island.sessions.section.pinned")),
                    ),
            )
        })
        .child(list);

    IslandPanel::new("sessions-island", body)
        .scroll(false)
        .header(header)
        .focused(focused)
}

/// Visible keyboard target order, kept identical to [`flatten_session_rows`].
pub(crate) fn visible_session_targets(
    vm: &SidebarViewModel,
    collapsed: &HashSet<String>,
) -> Vec<SessionListTarget> {
    let mut out = Vec::new();
    out.extend(
        vm.pinned
            .iter()
            .map(|row| SessionListTarget::Session(row.session_id.clone())),
    );
    for group in &vm.groups {
        if group.rows.is_empty() {
            continue;
        }
        let key = group_key(group);
        out.push(SessionListTarget::Directory(key.clone()));
        if !collapsed.contains(&key) {
            out.extend(
                group
                    .rows
                    .iter()
                    .map(|row| SessionListTarget::Session(row.session_id.clone())),
            );
        }
    }
    out
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
    on_new: &IdClickFactory,
    on_toggle: &IdClickFactory,
    session_menu: &SessionMenuFactory,
) -> Vec<(TreeRowSpec, ClickHandler, Option<ClickHandler>)> {
    let t = tokens();
    let mut out = Vec::new();

    for row in &vm.pinned {
        out.push(session_tree_row(row, on_open, session_menu, true));
    }

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
        let new_session = if key.is_empty() {
            None
        } else {
            Some(dir_new_session_button(&key, on_new))
        };

        let mute = ChromeTokens::hsla(t.muted_fg);
        let dir_icon = if expanded {
            ChromeIcon::FolderOpen
        } else {
            ChromeIcon::Folder
        };

        out.push((
            TreeRowSpec {
                id: SharedString::from(dir_id),
                depth: 0,
                has_children: true,
                expanded,
                selected: false,
                emphasized: false,
                keyboard_focused: false,
                show_guides: false,
                label: SharedString::from(group.label.clone()),
                label_color: Some(t.muted_fg_rgba()),
                leading: Some(row_leading(dir_icon, mute)),
                detail: None,
                accessory: new_session.map(TreeRowAccessory::Action),
                context_menu: None,
            },
            activate,
            Some(toggle),
        ));

        if !expanded {
            continue;
        }

        for row in &group.rows {
            out.push(session_tree_row(row, on_open, session_menu, false));
        }
    }

    out
}

fn dir_new_session_button(cwd: &str, on_new: &IdClickFactory) -> AnyElement {
    let handler = on_new(cwd.to_string());
    Button::new(SharedString::from(format!("new-session-{cwd}")))
        .icon(icon(
            ChromeIcon::Plus,
            IconSize::Meta,
            ChromeTokens::hsla(tokens().muted_fg),
        ))
        .tooltip(crate::t!("island.sessions.action.new"))
        .ghost()
        .small()
        .compact()
        .px(px(0.))
        .on_click(move |ev, window, cx| {
            cx.stop_propagation();
            handler(ev, window, cx);
        })
        .into_any_element()
}

fn session_tree_row(
    row: &SessionRow,
    on_open: &IdClickFactory,
    session_menu: &SessionMenuFactory,
    in_pinned_band: bool,
) -> (TreeRowSpec, ClickHandler, Option<ClickHandler>) {
    let t = tokens();
    let is_live = row.kind == SessionRowKind::LiveTarget;
    let is_pending = row.kind == SessionRowKind::PendingTarget;

    let label_color = if is_pending {
        Some(t.muted_fg_rgba())
    } else if is_live {
        None
    } else {
        Some(t.fg_rgba())
    };

    let leading_icon = if row.is_pinned || in_pinned_band {
        ChromeIcon::Pin
    } else {
        ChromeIcon::MessageSquare
    };

    let leading_color = if is_pending {
        ChromeTokens::hsla(t.muted_fg)
    } else if is_live {
        ChromeTokens::hsla(t.accent)
    } else if row.is_pinned || in_pinned_band {
        ChromeTokens::hsla(t.muted_fg)
    } else {
        ChromeTokens::hsla(t.fg)
    };

    let detail = if in_pinned_band && !row.cwd_hint.is_empty() {
        Some(
            text(TextRole::Meta)
                .text_color(t.muted_fg_rgba())
                .child(format!("· {}", row.cwd_hint))
                .into_any_element(),
        )
    } else {
        None
    };

    (
        TreeRowSpec {
            id: SharedString::from(format!("session-{}", row.session_id)),
            depth: if in_pinned_band { 0 } else { 1 },
            has_children: false,
            expanded: false,
            selected: is_live,
            emphasized: is_pending,
            keyboard_focused: false,
            // Sessions is a directory list, not the conversation Tree — indent
            // without vertical depth guides (ui-guidelines: guides are tree-only).
            show_guides: false,
            label: SharedString::from(row.label.clone()),
            label_color,
            leading: Some(row_leading(leading_icon, leading_color)),
            detail,
            accessory: Some(TreeRowAccessory::Meta(SharedString::from(
                row.message_count.to_string(),
            ))),
            context_menu: session_menu(row),
        },
        on_open(row.session_id.clone()),
        None,
    )
}

pub(crate) fn build_session_context_menu(
    row: &SessionRow,
    open: SessionMenuAction,
    rename: SessionMenuAction,
    toggle_pin: SessionMenuAction,
    delete: SessionMenuAction,
) -> Option<TreeContextMenuBuilder> {
    if row.kind == SessionRowKind::PendingTarget {
        return None;
    }
    let pin_label = if row.is_pinned {
        crate::t!("island.sessions.menu.unpin")
    } else {
        crate::t!("island.sessions.menu.pin")
    };
    let open = open.clone();
    let rename = rename.clone();
    let toggle_pin = toggle_pin.clone();
    let delete = delete.clone();
    Some(Rc::new(move |_request, _window, _cx| {
        ContextMenuSpec::new([
            ContextMenuItem::action(crate::t!("island.sessions.menu.open"), {
                let open = open.clone();
                move |window, cx| open(window, cx)
            }),
            ContextMenuItem::action(crate::t!("island.sessions.menu.rename"), {
                let rename = rename.clone();
                move |window, cx| rename(window, cx)
            }),
            ContextMenuItem::action(pin_label.clone(), {
                let toggle_pin = toggle_pin.clone();
                move |window, cx| toggle_pin(window, cx)
            }),
            ContextMenuItem::separator(),
            ContextMenuItem::action(crate::t!("island.sessions.menu.delete"), {
                let delete = delete.clone();
                move |window, cx| delete(window, cx)
            })
            .tone(ContextMenuItemTone::Destructive),
        ])
    }))
}

#[cfg(test)]
mod tests {
    use gpui::SharedString;

    use crate::projections::{SessionRow, SessionRowKind};
    use crate::shell::TreeRowAccessory;

    use super::{IdClickFactory, session_tree_row};

    fn noop_open_factory() -> IdClickFactory {
        Box::new(|_| Box::new(|_, _, _| {}))
    }

    fn noop_menu() -> super::SessionMenuFactory {
        Box::new(|_| None)
    }

    #[test]
    fn zero_message_session_keeps_a_meta_accessory() {
        let row = SessionRow {
            session_id: "empty".into(),
            label: "Empty session".into(),
            kind: SessionRowKind::Listed,
            message_count: 0,
            is_pinned: false,
            cwd_hint: String::new(),
        };

        let (spec, _, _) = session_tree_row(&row, &noop_open_factory(), &noop_menu(), false);
        assert!(!spec.show_guides, "sessions rows must not draw tree guides");
        match spec.accessory {
            Some(TreeRowAccessory::Meta(value)) => {
                assert_eq!(value, SharedString::from("0"));
            }
            Some(TreeRowAccessory::Action(_)) => panic!("count must be read-only metadata"),
            None => panic!("zero count must keep the accessory rail populated"),
        }
    }
}
