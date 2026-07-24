//! Timeline row body for IslandPanel's scroll viewport.
//!
//! Chat-primary presentation: You / Assistant headers, grouped speakers.
//! Rows render in timeline order from Client Core / hostd — no CoT bucketing
//! or cross-row reordering of thinking / tools / body.

use std::collections::HashSet;

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::theme::{
    ChromeIcon, ChromeTokens, DomainRole, TextRole, domain_role_hsla, domain_role_rgba, metrics,
    rotating_gear, row_leading, text, tokens,
};
use piko_chrome::components::markdown::render_selectable_markdown;
use piko_chrome::components::selection::{SelectableText, SelectionState, selectable_region};

use super::markdown_cache::TimelineMarkdownCache;
use super::tool::render_tool_chip;
use super::vm::{
    TimelineRow, TimelineRowKind, TimelineViewModel, VisualSender, group_timeline_rows,
};

pub(super) type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Reading-width conversation document (no scrollbar — IslandPanel owns that).
pub fn render_timeline_body(
    vm: &TimelineViewModel,
    markdown: &TimelineMarkdownCache,
    expanded: &HashSet<String>,
    allow_motion: bool,
    on_toggle: impl Fn(String) -> ClickHandler,
    cx: &mut App,
) -> impl IntoElement {
    let m = metrics();
    let groups = group_timeline_rows(&vm.rows);
    let mut group_els = Vec::with_capacity(groups.len());
    for group in groups {
        let sender = group[0].kind.visual_sender();
        let el = match sender {
            VisualSender::Assistant => {
                render_assistant_group(group, markdown, expanded, allow_motion, &on_toggle, cx)
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
                        markdown,
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

/// Render assistant-side rows in timeline order (tools and messages as given).
fn render_assistant_group(
    group: &[TimelineRow],
    markdown: &TimelineMarkdownCache,
    expanded: &HashSet<String>,
    allow_motion: bool,
    on_toggle: &impl Fn(String) -> ClickHandler,
    cx: &mut App,
) -> AnyElement {
    let m = metrics();
    let mut children: Vec<AnyElement> = Vec::new();
    let mut header_done = false;

    for row in group {
        match row.kind {
            TimelineRowKind::Tool => {
                children.push(render_tool_chip(
                    row,
                    expanded.contains(&row.id),
                    on_toggle(row.id.clone()),
                    markdown,
                    cx,
                ));
            }
            TimelineRowKind::Assistant | TimelineRowKind::Streaming => {
                let show_header = !header_done;
                header_done = true;
                children.push(render_assistant_row(
                    row,
                    show_header,
                    allow_motion,
                    markdown,
                    cx,
                ));
            }
            TimelineRowKind::User | TimelineRowKind::System => {}
        }
    }

    div()
        .flex()
        .flex_col()
        .gap(m.space_xs)
        .children(children)
        .into_any_element()
}

fn render_assistant_row(
    row: &TimelineRow,
    show_header: bool,
    allow_motion: bool,
    markdown: &TimelineMarkdownCache,
    cx: &mut App,
) -> AnyElement {
    let m = metrics();
    let selection = markdown
        .selection(&row.id)
        .expect("timeline selection state");
    let mut root = div()
        .id(SharedString::from(row.id.clone()))
        .flex()
        .flex_col()
        .gap(m.space_xs);

    if show_header {
        root = root.child(render_assistant_header(row, allow_motion));
    }
    if let Some(thinking) = row.thinking.as_ref().filter(|s| !s.trim().is_empty()) {
        root = root.child(render_thinking_block(
            &row.id,
            thinking,
            row.thinking_live,
            &selection,
        ));
    } else if row.thinking_live {
        root = root.child(render_thinking_block(&row.id, "", true, &selection));
    }
    if let Some(body) = render_message_body(row, markdown, &selection) {
        root = root.child(body);
    }
    selectable_region(
        SharedString::from(format!("{}-selection-region", row.id)),
        selection,
        crate::t!("island.timeline.menu.copy"),
        root,
        markdown.owner(),
        cx,
    )
}

fn render_thinking_block(
    row_id: &str,
    thinking: &str,
    live: bool,
    selection: &Entity<SelectionState>,
) -> AnyElement {
    let m = metrics();
    let inner = if thinking.trim().is_empty() && live {
        render_thinking_live_mark()
    } else {
        render_thinking_segment(row_id, thinking, live, selection)
    };
    div()
        .w_full()
        .flex()
        .flex_col()
        .pl(m.space_sm)
        .border_l_2()
        .border_color(domain_role_hsla(DomainRole::Thinking))
        .child(inner)
        .into_any_element()
}

fn render_assistant_header(row: &TimelineRow, allow_motion: bool) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let accent = domain_role_rgba(DomainRole::Assistant);
    let accent_hsla = domain_role_hsla(DomainRole::Assistant);
    div()
        .flex()
        .gap(m.space_sm)
        .items_center()
        .child(row_leading(ChromeIcon::Bot, accent_hsla))
        .child(
            text(TextRole::Meta)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(accent)
                .child(row.label.clone()),
        )
        .when(row.streaming, |d| {
            d.child(rotating_gear(ChromeTokens::hsla(t.muted_fg), allow_motion))
        })
        .into_any_element()
}

fn render_thinking_segment(
    row_id: &str,
    thinking: &str,
    live: bool,
    selection: &Entity<SelectionState>,
) -> AnyElement {
    let t = tokens();
    let color = if live {
        domain_role_rgba(DomainRole::Thinking)
    } else {
        t.muted_fg_rgba()
    };
    text(TextRole::Body)
        .w_full()
        .text_color(color)
        .child(selectable_plain(
            format!("{row_id}-thinking"),
            thinking.to_owned(),
            selection,
        ))
        .into_any_element()
}

fn render_thinking_live_mark() -> AnyElement {
    text(TextRole::Meta)
        .text_color(domain_role_rgba(DomainRole::Thinking))
        .child(crate::t!("island.timeline.thinking_live"))
        .into_any_element()
}

fn render_timeline_row(
    row: &TimelineRow,
    show_header: bool,
    _expanded: bool,
    _allow_motion: bool,
    _on_toggle: ClickHandler,
    markdown: &TimelineMarkdownCache,
    cx: &mut App,
) -> AnyElement {
    match row.kind {
        TimelineRowKind::System => render_system_row(row, markdown, cx),
        TimelineRowKind::User => render_user_row(row, show_header, markdown, cx),
        TimelineRowKind::Tool | TimelineRowKind::Assistant | TimelineRowKind::Streaming => {
            div().into_any_element()
        }
    }
}

fn render_system_row(
    row: &TimelineRow,
    markdown: &TimelineMarkdownCache,
    cx: &mut App,
) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let selection = markdown
        .selection(&row.id)
        .expect("timeline selection state");
    let value = if row.label.is_empty() {
        row.body.clone()
    } else {
        format!("{} · {}", row.label, row.body)
    };
    let root = div()
        .id(SharedString::from(row.id.clone()))
        .w_full()
        .flex()
        .justify_center()
        .py(m.space_xs)
        .child(
            text(TextRole::Meta)
                .text_color(t.muted_fg_rgba())
                .child(selectable_plain(
                    format!("{}-system", row.id),
                    value,
                    &selection,
                )),
        )
        .into_any_element();
    selectable_region(
        SharedString::from(format!("{}-selection-region", row.id)),
        selection,
        crate::t!("island.timeline.menu.copy"),
        root,
        markdown.owner(),
        cx,
    )
}

fn render_user_row(
    row: &TimelineRow,
    show_header: bool,
    markdown: &TimelineMarkdownCache,
    cx: &mut App,
) -> AnyElement {
    let m = metrics();
    let t = tokens();
    let accent = domain_role_rgba(DomainRole::User);
    let accent_hsla = domain_role_hsla(DomainRole::User);

    let selection = markdown
        .selection(&row.id)
        .expect("timeline selection state");
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
                .child(row_leading(ChromeIcon::User, accent_hsla))
                .child(
                    text(TextRole::Meta)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(accent)
                        .child(row.label.clone()),
                ),
        );
    }

    if let Some(body) = render_message_body(row, markdown, &selection) {
        root = root.child(
            div()
                .p(m.space_sm)
                .rounded_md()
                .bg(t.elevated_rgba())
                .child(body),
        );
    }

    selectable_region(
        SharedString::from(format!("{}-selection-region", row.id)),
        selection,
        crate::t!("island.timeline.menu.copy"),
        root,
        markdown.owner(),
        cx,
    )
}

fn render_message_body(
    row: &TimelineRow,
    markdown: &TimelineMarkdownCache,
    selection: &Entity<SelectionState>,
) -> Option<AnyElement> {
    if row.body.trim().is_empty() {
        return None;
    }
    let t = tokens();
    if row.render_markdown {
        markdown
            .document(&row.id)
            .map(|document| {
                render_selectable_markdown(
                    SharedString::from(format!("{}-body", row.id)),
                    document,
                    selection.clone(),
                )
            })
            .or_else(|| {
                Some(
                    text(TextRole::Body)
                        .w_full()
                        .text_color(t.fg_rgba())
                        .child(selectable_plain(
                            format!("{}-body", row.id),
                            row.body.clone(),
                            selection,
                        ))
                        .into_any_element(),
                )
            })
    } else {
        Some(
            text(TextRole::Body)
                .w_full()
                .text_color(t.fg_rgba())
                .child(selectable_plain(
                    format!("{}-body", row.id),
                    row.body.clone(),
                    selection,
                ))
                .into_any_element(),
        )
    }
}

pub(super) fn selectable_plain(
    id: impl Into<SharedString>,
    value: impl Into<SharedString>,
    selection: &Entity<SelectionState>,
) -> AnyElement {
    let id = id.into();
    let value = value.into();
    SelectableText::new(
        id.clone(),
        id,
        value.clone(),
        StyledText::new(value),
        selection.clone(),
    )
    .into_any_element()
}
