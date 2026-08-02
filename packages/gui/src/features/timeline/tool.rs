//! Expandable Timeline tool row.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use island::components::selection::selectable_region;

use crate::theme::{
    DomainRole, IconSize, IslandIcon, RoleAccent, TextRole, domain_role_hsla, icon, metrics, text,
    tokens,
};

use super::markdown_cache::TimelineMarkdownCache;
use super::render::{ClickHandler, selectable_plain};
use super::vm::{TimelineRow, ToolCardStatus};

/// Full-width terminal-style tool block; click expands detail downward and
/// grows content. Prototype variant of the compact tool chip.
pub(super) fn render_tool_chip(
    row: &TimelineRow,
    expanded: bool,
    on_toggle: ClickHandler,
    markdown: &TimelineMarkdownCache,
    cx: &mut App,
) -> AnyElement {
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
    let selection = markdown
        .selection(&row.id)
        .expect("timeline selection state");

    let tool_accent = domain_role_hsla(DomainRole::Tool);
    let chevron = if expanded {
        IslandIcon::ChevronDown
    } else {
        IslandIcon::ChevronRight
    };

    let block = div()
        .id(SharedString::from(format!("tool-block-{}", row.id)))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(m.space_sm)
        .py(m.space_xs)
        .pl(m.space_sm)
        .pr(m.space_sm)
        .rounded_md()
        .bg(t.elevated_rgba())
        .border_l_2()
        .border_color(tool_accent)
        .child(icon(
            IslandIcon::Wrench,
            IconSize::Meta,
            domain_role_hsla(DomainRole::Tool),
        ))
        .child(
            text(TextRole::Meta)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.fg_rgba())
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
        })
        .when(can_expand, |d| {
            d.child(div().flex_grow())
                .child(icon(chevron, IconSize::Meta, t.muted_fg_rgba()))
        })
        .when(can_expand, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(t.border_rgba()))
                .on_click(move |ev, window, cx| on_toggle(ev, window, cx))
        });

    let root = div()
        .id(SharedString::from(row.id.clone()))
        .w_full()
        .flex()
        .flex_col()
        .gap(m.space_xs)
        .child(block)
        .when(expanded, |d| {
            if let Some(detail) = &row.detail {
                d.child(
                    div()
                        .w_full()
                        .p(m.space_sm)
                        .rounded_md()
                        .bg(t.surface_rgba())
                        .max_h(px(160.))
                        .overflow_y_scrollbar()
                        .child(
                            text(TextRole::BodyMono)
                                .text_color(t.muted_fg_rgba())
                                .child(selectable_plain(
                                    format!("{}-tool-detail", row.id),
                                    detail.clone(),
                                    &selection,
                                )),
                        ),
                )
            } else {
                d
            }
        })
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
