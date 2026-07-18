//! GPUI render helpers for workbench rows (stateless IntoElement builders).

use std::collections::HashSet;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Disableable;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::text::TextView;

use crate::theme::{RoleAccent, metrics, tokens};

use super::{
    ActivityItem, ActivityItemKind, ActivityViewModel, ComposerViewModel, TimelineRow,
    TimelineRowKind, TimelineViewModel, ToolCardStatus,
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const ACTIVITY_MAX_HEIGHT: Pixels = px(180.);

#[allow(clippy::too_many_arguments)]
pub fn render_timeline_panel(
    vm: &TimelineViewModel,
    follow_bottom: bool,
    scroll_handle: &ScrollHandle,
    expanded_tools: &HashSet<String>,
    on_jump: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_toggle_tool: impl Fn(String) -> ClickHandler,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let m = metrics();
    let mut rows = Vec::with_capacity(vm.rows.len());
    for row in &vm.rows {
        let expanded = expanded_tools.contains(&row.id);
        let toggle = on_toggle_tool(row.id.clone());
        rows.push(render_timeline_row(row, expanded, toggle, window, cx));
    }

    div()
        .id("timeline-panel")
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .id("timeline-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .track_scroll(scroll_handle)
                .vertical_scrollbar(scroll_handle)
                .px(m.space_lg)
                .py(m.space_md)
                .child(
                    div().w_full().flex().justify_center().child(
                        div()
                            .w_full()
                            .max_w(m.reading_width)
                            .flex()
                            .flex_col()
                            .children(rows),
                    ),
                ),
        )
        .when(!follow_bottom && !vm.rows.is_empty(), |d| {
            d.child(
                div()
                    .px(m.space_sm)
                    .pb(m.space_sm)
                    .flex()
                    .justify_center()
                    .child(
                        Button::new("jump-latest")
                            .label("Jump to latest")
                            .ghost()
                            .small()
                            .compact()
                            .on_click(on_jump),
                    ),
            )
        })
}

fn render_timeline_row(
    row: &TimelineRow,
    expanded: bool,
    on_toggle: ClickHandler,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let t = tokens();
    let m = metrics();
    let accent = match row.kind {
        TimelineRowKind::User => t.role_accent(RoleAccent::User),
        TimelineRowKind::Assistant | TimelineRowKind::Streaming => {
            t.role_accent(RoleAccent::Assistant)
        }
        TimelineRowKind::Tool => t.role_accent(RoleAccent::Tool),
        TimelineRowKind::System => t.role_accent(RoleAccent::System),
    };
    let is_tool = row.kind == TimelineRowKind::Tool;
    let is_user = row.kind == TimelineRowKind::User;
    let is_system = row.kind == TimelineRowKind::System;

    let status_label = row.tool_status.map(|s| match s {
        ToolCardStatus::Running => "running",
        ToolCardStatus::Completed => "done",
        ToolCardStatus::Failed => "failed",
    });
    let status_accent = match row.tool_status {
        Some(ToolCardStatus::Running) => t.role_accent(RoleAccent::Info),
        Some(ToolCardStatus::Completed) => t.role_accent(RoleAccent::Success),
        Some(ToolCardStatus::Failed) => t.role_accent(RoleAccent::Danger),
        None => t.muted_fg_rgba(),
    };

    let body: AnyElement = if row.render_markdown && !row.streaming {
        TextView::markdown(
            SharedString::from(format!("md-{}", row.id)),
            row.body.clone(),
            window,
            cx,
        )
        .selectable(true)
        .text_size(m.body_size)
        .line_height(m.body_line_height)
        .into_any_element()
    } else {
        div()
            .text_size(m.body_size)
            .line_height(m.body_line_height)
            .text_color(if is_system {
                t.muted_fg_rgba()
            } else {
                t.fg_rgba()
            })
            .child(row.body.clone())
            .into_any_element()
    };

    div()
        .id(SharedString::from(row.id.clone()))
        .flex()
        .flex_col()
        .gap(m.space_xs)
        .px(m.space_md)
        .py(m.space_md)
        .mb(m.space_sm)
        .when(is_user || is_tool, |d| d.rounded_md().bg(t.elevated_rgba()))
        .when(is_system, |d| d.opacity(0.82))
        .child(
            div()
                .flex()
                .gap(m.space_sm)
                .items_center()
                .child(
                    div()
                        .w(px(6.))
                        .h(px(6.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(accent),
                )
                .child(
                    div()
                        .text_size(m.label_size)
                        .line_height(m.label_line_height)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(accent)
                        .child(row.label.clone()),
                )
                .when_some(status_label, |d, s| {
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(m.space_xs)
                            .text_size(m.meta_size)
                            .line_height(m.meta_line_height)
                            .text_color(status_accent)
                            .child(div().w(px(5.)).h(px(5.)).rounded_full().bg(status_accent))
                            .child(s),
                    )
                })
                .when(row.streaming, |d| {
                    d.child(
                        div()
                            .text_size(m.meta_size)
                            .line_height(m.meta_line_height)
                            .text_color(t.role_accent(RoleAccent::Thinking))
                            .child("streaming"),
                    )
                })
                .when(row.detail.is_some(), |d| {
                    d.child(div().flex_1()).child(
                        Button::new(SharedString::from(format!("toggle-{}", row.id)))
                            .label(if expanded { "Hide" } else { "Detail" })
                            .ghost()
                            .small()
                            .compact()
                            .on_click(move |ev, window, cx| on_toggle(ev, window, cx)),
                    )
                }),
        )
        .child(body)
        .when(expanded, |d| {
            if let Some(detail) = &row.detail {
                d.child(
                    div()
                        .mt(m.space_xs)
                        .p(m.space_sm)
                        .rounded_md()
                        .bg(t.surface_rgba())
                        .max_h(px(160.))
                        .overflow_y_scrollbar()
                        .child(
                            div()
                                .text_size(m.meta_size)
                                .line_height(m.meta_line_height)
                                .font_family("monospace")
                                .text_color(t.muted_fg_rgba())
                                .child(detail.clone()),
                        ),
                )
            } else {
                d
            }
        })
        .into_any_element()
}

/// Activity Center: quiet header + optional capped expansion.
pub fn render_activity_center(
    vm: &ActivityViewModel,
    expanded: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_item: impl Fn(ActivityItem) -> ClickHandler,
) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .id("activity-center")
        .mb(m.space_xs)
        .flex()
        .flex_col()
        .child(
            div()
                .id("activity-header")
                .h(px(32.))
                .px(m.space_sm)
                .flex()
                .items_center()
                .gap(m.space_sm)
                .rounded_md()
                .hover(|style| style.bg(t.elevated_rgba()))
                .cursor_pointer()
                .on_click(on_toggle)
                .child(div().w(px(6.)).h(px(6.)).flex_shrink_0().rounded_full().bg(
                    if vm.has_actionable {
                        t.role_accent(RoleAccent::Warning)
                    } else if vm.show_stop {
                        t.role_accent(RoleAccent::Info)
                    } else {
                        t.muted_fg_rgba()
                    },
                ))
                .child(
                    div()
                        .w(px(12.))
                        .text_center()
                        .text_size(m.meta_size)
                        .line_height(m.meta_line_height)
                        .text_color(if vm.has_actionable {
                            t.role_accent(RoleAccent::Warning)
                        } else {
                            t.muted_fg_rgba()
                        })
                        .child(if expanded { "▾" } else { "▸" }),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(m.meta_size)
                        .line_height(m.meta_line_height)
                        .text_color(if vm.has_actionable {
                            t.role_accent(RoleAccent::Warning)
                        } else {
                            t.muted_fg_rgba()
                        })
                        .child(vm.summary.clone()),
                ),
        )
        .when(expanded && !vm.items.is_empty(), |d| {
            d.child(
                div()
                    .id("activity-body")
                    .max_h(ACTIVITY_MAX_HEIGHT)
                    .overflow_y_scroll()
                    .mt(m.space_xs)
                    .p(m.space_xs)
                    .rounded_md()
                    .bg(t.elevated_rgba())
                    .flex()
                    .flex_col()
                    .gap(m.space_xs)
                    .children(vm.items.iter().map(|item| {
                        let handler = on_item(item.clone());
                        render_activity_item(item, handler)
                    })),
            )
        })
}

fn render_activity_item(item: &ActivityItem, on_click: ClickHandler) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    let accent = match item.kind {
        ActivityItemKind::Warning | ActivityItemKind::ToolFailed => {
            t.role_accent(RoleAccent::Danger)
        }
        ActivityItemKind::Approval | ActivityItemKind::Interaction => {
            t.role_accent(RoleAccent::Warning)
        }
        ActivityItemKind::TurnRunning | ActivityItemKind::ToolRunning => {
            t.role_accent(RoleAccent::Info)
        }
        ActivityItemKind::UnreadReport => t.role_accent(RoleAccent::Accent),
        ActivityItemKind::TurnQueued => t.muted_fg_rgba(),
    };
    div()
        .id(SharedString::from(item.id.clone()))
        .px(m.space_sm)
        .py(m.space_xs)
        .rounded_md()
        .flex()
        .items_center()
        .gap(m.space_sm)
        .hover(|s| s.bg(t.surface_rgba()))
        .cursor_pointer()
        .on_click(move |ev, window, cx| on_click(ev, window, cx))
        .child(
            div()
                .w(px(5.))
                .h(px(5.))
                .flex_shrink_0()
                .rounded_full()
                .bg(accent),
        )
        .child(
            div()
                .text_size(m.meta_size)
                .line_height(m.meta_line_height)
                .text_color(accent)
                .child(item.label.clone()),
        )
}

pub fn render_composer_panel(
    vm: &ComposerViewModel,
    input: &Entity<InputState>,
    on_send: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_stop: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_cycle_model: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_cycle_thinking: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .id("composer-panel")
        .p(m.space_sm)
        .flex()
        .flex_col()
        .gap(m.space_sm)
        .child(Input::new(input).bg(t.elevated_rgba()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(m.space_sm)
                .child(
                    div()
                        .text_size(m.label_size)
                        .line_height(m.label_line_height)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.role_accent(RoleAccent::Accent))
                        .child(format!("@{}", vm.target_label)),
                )
                .child(
                    Button::new("cycle-model")
                        .label(vm.model_label.clone())
                        .ghost()
                        .small()
                        .compact()
                        .text_color(t.muted_fg_rgba())
                        .disabled(!vm.can_cycle_model)
                        .on_click(on_cycle_model),
                )
                .child(
                    Button::new("cycle-thinking")
                        .label(format!("think:{}", vm.thinking_label))
                        .ghost()
                        .small()
                        .compact()
                        .text_color(t.muted_fg_rgba())
                        .disabled(!vm.can_cycle_thinking)
                        .on_click(on_cycle_thinking),
                )
                .child(div().flex_1())
                .when(vm.show_stop, |d| {
                    d.child(
                        Button::new("stop-turn")
                            .label("Stop")
                            .ghost()
                            .small()
                            .compact()
                            .text_color(t.role_accent(RoleAccent::Danger))
                            .on_click(on_stop),
                    )
                })
                .child(
                    Button::new("send-turn")
                        .primary()
                        .small()
                        .compact()
                        .label("Send")
                        .disabled(!vm.can_send)
                        .on_click(on_send),
                ),
        )
}
