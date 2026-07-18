//! Composer island: Activity Center + input panel.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Disableable;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};

use crate::theme::{RoleAccent, metrics, tokens};

use super::activity_vm::{ActivityItem, ActivityItemKind, ActivityViewModel};
use super::vm::ComposerViewModel;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const ACTIVITY_MAX_HEIGHT: Pixels = px(180.);

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
        .w_full()
        .mb(m.space_xs)
        .flex()
        .flex_col()
        .child(
            div()
                .id("activity-header")
                .w_full()
                .h(px(32.))
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
        .w_full()
        .flex()
        .flex_col()
        .gap(m.space_sm)
        .child(Input::new(input).w_full().bg(t.elevated_rgba()))
        .child(
            div()
                .w_full()
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
