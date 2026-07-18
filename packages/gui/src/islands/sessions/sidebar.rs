//! Session sidebar panel body for [`super::SessionsIsland`].

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};

use crate::chrome::{IslandHeader, IslandPanel, IslandPlaceholder};
use crate::projections::{SessionRow, SessionRowKind, SidebarGroup, SidebarViewModel};
use crate::theme::{metrics, tokens};

pub(crate) type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
pub(crate) type IdClickFactory = Box<dyn Fn(String) -> ClickHandler>;

pub(crate) fn render_sidebar_panel(
    vm: &SidebarViewModel,
    on_new_session: ClickHandler,
    on_open_session: IdClickFactory,
    focused: bool,
) -> IslandPanel {
    let new_session = Button::new("new-session")
        .label("+")
        .tooltip("New Session")
        .ghost()
        .small()
        .compact()
        .px(px(0.))
        .on_click(move |ev, window, cx| on_new_session(ev, window, cx))
        .into_any_element();

    let has_rows = vm.groups.iter().any(|g| !g.rows.is_empty());
    let header = IslandHeader::title_with_actions("Sessions", [new_session]);

    if !has_rows {
        return IslandPanel::empty(
            "sessions-island",
            IslandPlaceholder::new("No sessions")
                .icon("○")
                .subtitle("Create a session to get started"),
        )
        .header(header)
        .focused(focused);
    }

    let body = div().w_full().flex().flex_col().children(
        vm.groups
            .iter()
            .enumerate()
            .map(|(gix, group)| render_cwd_group(gix, group, &on_open_session)),
    );

    IslandPanel::new("sessions-island", body)
        .header(header)
        .focused(focused)
}

fn render_cwd_group(
    gix: usize,
    group: &SidebarGroup,
    on_open: &IdClickFactory,
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();
    let group_id = if group.cwd.is_empty() {
        format!("session-group-pending-{gix}")
    } else {
        format!("session-group-{}", group.cwd)
    };
    div()
        .id(SharedString::from(group_id))
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .px(m.space_sm)
                .pt(if gix == 0 { m.space_xs } else { m.space_sm })
                .pb(m.space_xs)
                .text_size(m.meta_size)
                .line_height(m.meta_line_height)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.muted_fg_rgba())
                .truncate()
                .child(group.label.clone()),
        )
        .children(group.rows.iter().enumerate().map(|(rix, row)| {
            let handler = on_open(row.session_id.clone());
            render_session_row(gix, rix, row, handler)
        }))
}

fn render_session_row(
    gix: usize,
    rix: usize,
    row: &SessionRow,
    on_click: ClickHandler,
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();
    let is_live = row.kind == SessionRowKind::LiveTarget;
    let is_pending = row.kind == SessionRowKind::PendingTarget;

    div()
        .id(SharedString::from(format!("session-row-{gix}-{rix}")))
        .h(px(32.))
        .w_full()
        .px(m.space_sm)
        .flex()
        .items_center()
        .gap(m.space_sm)
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(t.elevated_rgba()))
        .when(is_live || is_pending, |d| d.bg(t.elevated_rgba()))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(m.label_size)
                .line_height(m.label_line_height)
                .when(is_live, |d| {
                    d.font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.role_accent(crate::theme::RoleAccent::Accent))
                })
                .when(is_pending, |d| d.text_color(t.muted_fg_rgba()))
                .child(row.label.clone()),
        )
        .when(row.message_count > 0, |d| {
            d.child(
                div()
                    .flex_shrink_0()
                    .text_size(m.meta_size)
                    .line_height(m.meta_line_height)
                    .text_color(t.muted_fg_rgba())
                    .child(row.message_count.to_string()),
            )
        })
        .on_click(move |ev, window, cx| on_click(ev, window, cx))
}
