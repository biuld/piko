//! Timeline row body for IslandPanel's scroll viewport.
//!
//! Chat-primary presentation: You / Assistant headers, grouped speakers,
//! block-interleaved thinking / left-aligned tool chips / assistant prose.

use std::collections::HashSet;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::scroll::ScrollableElement;

use crate::theme::{
    IconSize, PikoIcon, PikoTokens, RoleAccent, TextRole, body_markdown, icon, metrics,
    rotating_gear, row_leading, text, tokens,
};

use super::vm::{
    TimelineRow, TimelineRowKind, TimelineViewModel, ToolCardStatus, VisualSender,
    group_timeline_rows,
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Reading-width conversation document (no scrollbar — IslandPanel owns that).
pub fn render_timeline_body(
    vm: &TimelineViewModel,
    expanded: &HashSet<String>,
    allow_motion: bool,
    on_toggle: impl Fn(String) -> ClickHandler,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let m = metrics();
    let groups = group_timeline_rows(&vm.rows);
    let mut group_els = Vec::with_capacity(groups.len());
    for group in groups {
        let sender = group[0].kind.visual_sender();
        let el = match sender {
            VisualSender::Assistant => {
                render_assistant_group(group, expanded, allow_motion, &on_toggle, window, cx)
            }
            VisualSender::You | VisualSender::System => {
                let mut items = Vec::with_capacity(group.len());
                for (index, row) in group.iter().enumerate() {
                    let show_header = index == 0 && sender == VisualSender::You;
                    items.push(render_timeline_row(
                        row,
                        show_header,
                        expanded.contains(&row.id),
                        allow_motion,
                        on_toggle(row.id.clone()),
                        window,
                        cx,
                    ));
                }
                div()
                    .flex()
                    .flex_col()
                    .gap(m.space_xs)
                    .children(items)
                    .into_any_element()
            }
        };
        group_els.push(el);
    }

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
                .gap(m.space_md)
                .child(
                    div()
                        .id("timeline-pad-top")
                        .w_full()
                        .h(top_pad)
                        .flex_shrink_0(),
                )
                .children(group_els)
                .child(
                    div()
                        .id("timeline-pad-bottom")
                        .w_full()
                        .h(bottom_pad)
                        .flex_shrink_0(),
                ),
        )
}

/// Block-interleave thinking / tools / body within one Assistant speaker group.
fn render_assistant_group(
    group: &[TimelineRow],
    expanded: &HashSet<String>,
    allow_motion: bool,
    on_toggle: &impl Fn(String) -> ClickHandler,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let has_thinking = group
        .iter()
        .any(|r| r.thinking.as_ref().is_some_and(|s| !s.trim().is_empty()) || r.thinking_live);

    let mut header: Option<AnyElement> = None;
    let mut cot: Vec<AnyElement> = Vec::new();
    let mut main: Vec<AnyElement> = Vec::new();
    let mut seen_body = false;
    let mut header_done = false;

    for row in group {
        match row.kind {
            TimelineRowKind::Tool => {
                let chip =
                    render_tool_chip(row, expanded.contains(&row.id), on_toggle(row.id.clone()));
                if has_thinking && !seen_body {
                    cot.push(chip);
                } else {
                    main.push(chip);
                }
            }
            TimelineRowKind::Assistant | TimelineRowKind::Streaming => {
                if !header_done {
                    header = Some(render_assistant_header(row, allow_motion));
                    header_done = true;
                }
                if let Some(thinking) = row.thinking.as_ref().filter(|s| !s.trim().is_empty()) {
                    cot.push(render_thinking_segment(thinking, row.thinking_live));
                } else if row.thinking_live {
                    cot.push(render_thinking_live_mark());
                }
                if let Some(body) = render_message_body(row, window, cx) {
                    seen_body = true;
                    main.push(body);
                }
            }
            TimelineRowKind::User | TimelineRowKind::System => {}
        }
    }

    let mut root = div().flex().flex_col().gap(m.space_xs);
    if let Some(h) = header {
        root = root.child(h);
    }
    if !cot.is_empty() {
        root = root.child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(m.space_xs)
                .pl(m.space_sm)
                .border_l_2()
                .border_color(t.role_accent_hsla(RoleAccent::Thinking))
                .children(cot),
        );
    }
    if !main.is_empty() {
        root = root.child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(m.space_xs)
                .children(main),
        );
    }
    root.into_any_element()
}

fn render_assistant_header(row: &TimelineRow, allow_motion: bool) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let accent = t.role_accent(RoleAccent::Assistant);
    let accent_hsla = t.role_accent_hsla(RoleAccent::Assistant);
    div()
        .flex()
        .gap(m.space_sm)
        .items_center()
        .child(row_leading(PikoIcon::Bot, accent_hsla))
        .child(
            text(TextRole::Meta)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(accent)
                .child(row.label.clone()),
        )
        .when(row.streaming, |d| {
            d.child(rotating_gear(PikoTokens::hsla(t.muted_fg), allow_motion))
        })
        .into_any_element()
}

fn render_thinking_segment(thinking: &str, live: bool) -> AnyElement {
    let t = tokens();
    let color = if live {
        t.role_accent(RoleAccent::Thinking)
    } else {
        t.muted_fg_rgba()
    };
    text(TextRole::Body)
        .w_full()
        .text_color(color)
        .child(thinking.to_string())
        .into_any_element()
}

fn render_thinking_live_mark() -> AnyElement {
    let t = tokens();
    text(TextRole::Meta)
        .text_color(t.role_accent(RoleAccent::Thinking))
        .child(crate::t!("island.timeline.thinking_live"))
        .into_any_element()
}

fn render_timeline_row(
    row: &TimelineRow,
    show_header: bool,
    _expanded: bool,
    _allow_motion: bool,
    _on_toggle: ClickHandler,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    match row.kind {
        TimelineRowKind::System => render_system_row(row),
        TimelineRowKind::User => render_user_row(row, show_header, window, cx),
        // Assistant groups go through render_assistant_group; tools too.
        TimelineRowKind::Tool | TimelineRowKind::Assistant | TimelineRowKind::Streaming => {
            div().into_any_element()
        }
    }
}

fn render_system_row(row: &TimelineRow) -> AnyElement {
    let m = metrics();
    let t = tokens();
    div()
        .id(SharedString::from(row.id.clone()))
        .w_full()
        .flex()
        .justify_center()
        .py(m.space_xs)
        .child(
            text(TextRole::Meta)
                .text_color(t.muted_fg_rgba())
                .child(if row.label.is_empty() {
                    row.body.clone()
                } else {
                    format!("{} · {}", row.label, row.body)
                }),
        )
        .into_any_element()
}

fn render_user_row(
    row: &TimelineRow,
    show_header: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let accent = t.role_accent(RoleAccent::User);
    let accent_hsla = t.role_accent_hsla(RoleAccent::User);

    let mut root = div()
        .id(SharedString::from(row.id.clone()))
        .flex()
        .flex_col()
        .gap(m.space_xs);

    if show_header {
        root = root.child(
            div()
                .flex()
                .gap(m.space_sm)
                .items_center()
                .child(row_leading(PikoIcon::User, accent_hsla))
                .child(
                    text(TextRole::Meta)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(accent)
                        .child(row.label.clone()),
                ),
        );
    }

    if let Some(body) = render_message_body(row, window, cx) {
        root = root.child(
            div()
                .p(m.space_sm)
                .rounded_md()
                .bg(t.elevated_rgba())
                .child(body),
        );
    }

    root.into_any_element()
}

fn render_message_body(row: &TimelineRow, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    if row.body.trim().is_empty() {
        return None;
    }
    let t = tokens();
    if row.render_markdown {
        Some(
            div()
                .w_full()
                .min_w_0()
                .child(
                    body_markdown(
                        SharedString::from(format!("md-{}", row.id)),
                        row.body.clone(),
                        window,
                        cx,
                    )
                    .selectable(true),
                )
                .into_any_element(),
        )
    } else {
        Some(
            text(TextRole::Body)
                .w_full()
                .text_color(t.fg_rgba())
                .child(row.body.clone())
                .into_any_element(),
        )
    }
}

/// Left-aligned tool chip; click expands detail downward and grows content.
fn render_tool_chip(row: &TimelineRow, expanded: bool, on_toggle: ClickHandler) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let status = row.tool_status.unwrap_or(ToolCardStatus::Completed);
    let status_color = match status {
        ToolCardStatus::Running => t.role_accent(RoleAccent::Info),
        ToolCardStatus::Completed => t.role_accent(RoleAccent::Success),
        ToolCardStatus::Failed => t.role_accent(RoleAccent::Danger),
    };
    let status_label = match status {
        ToolCardStatus::Running => crate::t!("island.timeline.tool.running"),
        ToolCardStatus::Completed => crate::t!("island.timeline.tool.done"),
        ToolCardStatus::Failed => crate::t!("island.timeline.tool.failed"),
    };
    let can_expand = row.detail.is_some();

    div()
        .id(SharedString::from(row.id.clone()))
        .flex()
        .flex_col()
        .gap(m.space_xs)
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("tool-chip-{}", row.id)))
                .flex()
                .flex_row()
                .items_center()
                .gap(m.space_sm)
                .px(m.space_sm)
                .py(m.space_xs)
                .rounded_md()
                .when(can_expand, |d| {
                    d.cursor_pointer()
                        .hover(|s| s.bg(t.elevated_rgba()))
                        .on_click(move |ev, window, cx| on_toggle(ev, window, cx))
                })
                .child(icon(
                    PikoIcon::Wrench,
                    IconSize::Meta,
                    t.role_accent_hsla(RoleAccent::Tool),
                ))
                .child(
                    text(TextRole::Meta)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.muted_fg_rgba())
                        .child(row.label.clone()),
                )
                .child(
                    text(TextRole::Meta)
                        .text_color(status_color)
                        .child(format!("· {status_label}")),
                )
                .when(!row.body.is_empty() && !expanded, |d| {
                    d.child(
                        text(TextRole::Meta)
                            .text_color(t.muted_fg_rgba())
                            .child(format!("— {}", row.body)),
                    )
                }),
        )
        .when(expanded, |d| {
            if let Some(detail) = &row.detail {
                d.child(
                    div()
                        .w_full()
                        .p(m.space_sm)
                        .rounded_md()
                        .bg(t.elevated_rgba())
                        .max_h(px(160.))
                        .overflow_y_scrollbar()
                        .child(
                            text(TextRole::BodyMono)
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
