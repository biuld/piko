//! Composer island: Activity Center + input panel.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Disableable;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};

use crate::theme::{
    ChromeIcon, ChromeTokens, IconSize, RoleAccent, disclosure, icon, metrics, row_leading, tokens,
};

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
    let header_color = if vm.has_actionable {
        t.role_accent_hsla(RoleAccent::Warning)
    } else if vm.show_stop {
        t.role_accent_hsla(RoleAccent::Info)
    } else {
        ChromeTokens::hsla(t.muted_fg)
    };
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
                .px(m.space_sm)
                .rounded_md()
                .hover(|style| style.bg(t.elevated_rgba()))
                .cursor_pointer()
                .on_click(on_toggle)
                .child(row_leading(ChromeIcon::Activity, header_color))
                .child(
                    crate::theme::text(crate::theme::TextRole::Meta)
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(if vm.has_actionable {
                            t.role_accent(RoleAccent::Warning)
                        } else if vm.show_stop {
                            t.role_accent(RoleAccent::Info)
                        } else {
                            t.muted_fg_rgba()
                        })
                        .child(vm.summary.clone()),
                )
                .child(disclosure(expanded, ChromeTokens::hsla(t.muted_fg))),
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
    let (icon_name, accent_role) = activity_item_icon(item.kind);
    let accent = match accent_role {
        Some(role) => t.role_accent(role),
        None => t.muted_fg_rgba(),
    };
    let accent_hsla = match accent_role {
        Some(role) => t.role_accent_hsla(role),
        None => ChromeTokens::hsla(t.muted_fg),
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
        .child(row_leading(icon_name, accent_hsla))
        .child(
            crate::theme::text(crate::theme::TextRole::Meta)
                .text_color(accent)
                .child(item.label.clone()),
        )
}

fn activity_item_icon(kind: ActivityItemKind) -> (ChromeIcon, Option<RoleAccent>) {
    match kind {
        ActivityItemKind::Approval | ActivityItemKind::Interaction => {
            (ChromeIcon::Bell, Some(RoleAccent::Warning))
        }
        ActivityItemKind::TurnRunning => (ChromeIcon::Activity, Some(RoleAccent::Info)),
        ActivityItemKind::TurnQueued => (ChromeIcon::CircleDashed, None),
        ActivityItemKind::ToolRunning => (ChromeIcon::Wrench, Some(RoleAccent::Info)),
        ActivityItemKind::ToolFailed | ActivityItemKind::Warning => {
            (ChromeIcon::TriangleAlert, Some(RoleAccent::Danger))
        }
        ActivityItemKind::UnreadReport => (ChromeIcon::Inbox, Some(RoleAccent::Accent)),
    }
}

pub fn render_composer_panel(
    vm: &ComposerViewModel,
    input: &Entity<InputState>,
    on_send: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_stop: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    let mute = ChromeTokens::hsla(t.muted_fg);
    let accent = t.role_accent_hsla(RoleAccent::Accent);
    let danger = t.role_accent_hsla(RoleAccent::Danger);
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
                .child(row_leading(ChromeIcon::Bot, accent))
                .child(
                    crate::theme::text(crate::theme::TextRole::Label)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.role_accent(RoleAccent::Accent))
                        .child(vm.target_label.clone()),
                )
                .child(row_leading(ChromeIcon::Cpu, mute))
                .child(
                    crate::theme::text(crate::theme::TextRole::Meta)
                        .text_color(t.muted_fg_rgba())
                        .child(vm.model_label.clone()),
                )
                .child(row_leading(ChromeIcon::Brain, mute))
                .child(
                    crate::theme::text(crate::theme::TextRole::Meta)
                        .text_color(t.muted_fg_rgba())
                        .child(crate::t!(
                            "composer.thinking.prefix",
                            level = vm.thinking_label.as_str()
                        )),
                )
                .child(div().flex_1())
                .when(vm.show_stop, |d| {
                    d.child(
                        Button::new("stop-turn")
                            .icon(icon(ChromeIcon::CircleStop, IconSize::Meta, danger))
                            .label(crate::t!("composer.action.stop"))
                            .ghost()
                            .small()
                            .compact()
                            .text_color(t.role_accent(RoleAccent::Danger))
                            .on_click(on_stop),
                    )
                })
                .child(
                    Button::new("send-turn")
                        .icon(icon(
                            ChromeIcon::Send,
                            IconSize::Meta,
                            ChromeTokens::hsla(t.fg),
                        ))
                        .primary()
                        .small()
                        .compact()
                        .label(crate::t!("composer.action.send"))
                        .disabled(!vm.can_send)
                        .on_click(on_send),
                ),
        )
}
