//! Shared Settings form primitives (theme tokens + typography roles).

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Sizable;
use gpui_component::switch::Switch;

use crate::theme::{TextRole, label_text, metrics, text, tokens};

pub fn section_lede(body: impl Into<SharedString>) -> impl IntoElement {
    text(TextRole::Body)
        .text_color(tokens().muted_fg_rgba())
        .child(body.into())
}

pub fn setting_group(children: impl IntoElement) -> impl IntoElement {
    let m = metrics();
    let t = tokens();
    div()
        .flex()
        .flex_col()
        .gap(m.space_sm)
        .p(m.space_md)
        .rounded(m.island_radius)
        .bg(t.elevated_rgba())
        .child(children)
}

pub fn setting_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    detail: Option<SharedString>,
    control: impl IntoElement,
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap(m.space_xs)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(m.space_md)
                .child(
                    label_text(false)
                        .text_color(t.fg_rgba())
                        .child(label.into()),
                )
                .child(control),
        )
        .when_some(detail, |row, detail| {
            row.child(
                text(TextRole::Meta)
                    .text_color(t.muted_fg_rgba())
                    .child(detail),
            )
        })
}

pub fn bool_switch(
    id: impl Into<ElementId>,
    checked: bool,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Switch::new(id)
        .checked(checked)
        .small()
        .on_click(move |checked, window, cx| on_change(*checked, window, cx))
}

pub fn value_chip(label: impl Into<SharedString>) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .px(m.space_sm)
        .py(px(2.))
        .rounded_sm()
        .bg(t.canvas_rgba())
        .child(
            text(TextRole::Meta)
                .text_color(t.fg_rgba())
                .child(label.into()),
        )
}

pub fn selectable_chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .id(id)
        .px(m.space_sm)
        .py(px(2.))
        .rounded_sm()
        .cursor_pointer()
        .when(selected, |chip| chip.bg(t.canvas_rgba()))
        .when(!selected, |chip| {
            chip.hover(|style| style.bg(t.canvas_rgba()))
        })
        .on_click(on_click)
        .child(
            text(TextRole::Meta)
                .text_color(if selected {
                    t.role_accent(crate::theme::RoleAccent::Accent)
                } else {
                    t.muted_fg_rgba()
                })
                .child(label.into()),
        )
}

pub fn model_row(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .id(id)
        .px(m.space_sm)
        .py(m.space_xs)
        .rounded_md()
        .cursor_pointer()
        .when(selected, |row| row.bg(t.canvas_rgba()))
        .when(!selected, |row| {
            row.hover(|style| style.bg(t.canvas_rgba()))
        })
        .on_click(on_click)
        .child(
            label_text(selected)
                .text_color(if selected {
                    t.role_accent(crate::theme::RoleAccent::Accent)
                } else {
                    t.fg_rgba()
                })
                .child(title.into()),
        )
        .child(
            text(TextRole::Meta)
                .text_color(t.muted_fg_rgba())
                .child(detail.into()),
        )
}

pub fn status_badge(label: impl Into<SharedString>, ok: bool) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    let color = if ok {
        t.role_accent(crate::theme::RoleAccent::Success)
    } else {
        t.muted_fg_rgba()
    };
    div()
        .px(m.space_sm)
        .py(px(2.))
        .rounded_sm()
        .bg(t.canvas_rgba())
        .child(text(TextRole::Meta).text_color(color).child(label.into()))
}

pub fn text_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let t = tokens();
    div().id(id).cursor_pointer().on_click(on_click).child(
        text(TextRole::Label)
            .text_color(t.role_accent(crate::theme::RoleAccent::Accent))
            .child(label.into()),
    )
}
