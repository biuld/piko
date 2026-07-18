//! Timeline row body for IslandPanel's scroll viewport.

use std::collections::HashSet;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;
use gpui_component::text::TextView;

use crate::theme::{RoleAccent, metrics, tokens};

use super::vm::{TimelineRow, TimelineRowKind, TimelineViewModel, ToolCardStatus};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Reading-width conversation document (no scrollbar — IslandPanel owns that).
pub fn render_timeline_body(
    vm: &TimelineViewModel,
    expanded_tools: &HashSet<String>,
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

    // Top spacer matches island header title inset so headerless Timeline
    // aligns with titled panels (Sessions / Agents / Tree). Bottom keeps
    // space_lg so scroll-to-bottom still leaves room under the last row.
    // Spacers (not container padding): padding on the scroll parent is easy
    // to clip.
    let top_pad = m.panel_header_title_inset;
    let bottom_pad = m.space_lg;

    div()
        .id("timeline-body")
        .w_full()
        .px(m.space_lg)
        .flex()
        .justify_center()
        .child(
            div()
                .w_full()
                .max_w(m.reading_width)
                .flex()
                .flex_col()
                .gap(m.space_sm)
                .child(
                    div()
                        .id("timeline-pad-top")
                        .w_full()
                        .h(top_pad)
                        .flex_shrink_0(),
                )
                .children(rows)
                .child(
                    div()
                        .id("timeline-pad-bottom")
                        .w_full()
                        .h(bottom_pad)
                        .flex_shrink_0(),
                ),
        )
}

fn render_timeline_row(
    row: &TimelineRow,
    expanded: bool,
    on_toggle: ClickHandler,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let (accent, bg) = match row.kind {
        TimelineRowKind::User => (t.role_accent(RoleAccent::User), t.canvas_rgba()),
        TimelineRowKind::Assistant | TimelineRowKind::Streaming => {
            (t.role_accent(RoleAccent::Assistant), t.surface_rgba())
        }
        TimelineRowKind::Tool => (t.role_accent(RoleAccent::Tool), t.surface_rgba()),
        TimelineRowKind::System => (t.role_accent(RoleAccent::System), t.surface_rgba()),
    };

    let status_label = row.tool_status.map(|s| match s {
        ToolCardStatus::Running => "running",
        ToolCardStatus::Completed => "done",
        ToolCardStatus::Failed => "failed",
    });

    let body: AnyElement = if row.render_markdown {
        TextView::markdown(
            SharedString::from(format!("md-{}", row.id)),
            row.body.clone(),
            window,
            cx,
        )
        .selectable(true)
        .into_any_element()
    } else {
        div()
            .text_size(m.body_size)
            .line_height(m.body_line_height)
            .text_color(t.fg_rgba())
            .child(row.body.clone())
            .into_any_element()
    };

    div()
        .id(SharedString::from(row.id.clone()))
        .flex()
        .flex_col()
        .gap(m.space_xs)
        .p(m.space_sm)
        .rounded_md()
        .bg(bg)
        .border_l_2()
        .border_color(accent)
        .child(
            div()
                .flex()
                .gap(m.space_sm)
                .items_center()
                .child(
                    div()
                        .text_size(m.meta_size)
                        .line_height(m.meta_line_height)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(accent)
                        .child(row.label.clone()),
                )
                .when_some(status_label, |d, s| {
                    d.child(
                        div()
                            .text_size(m.meta_size)
                            .line_height(m.meta_line_height)
                            .text_color(t.muted_fg_rgba())
                            .child(s),
                    )
                })
                .when(row.streaming, |d| {
                    d.child(
                        div()
                            .text_size(m.meta_size)
                            .line_height(m.meta_line_height)
                            .text_color(t.muted_fg_rgba())
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
                        .bg(t.elevated_rgba())
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
