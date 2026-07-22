//! Command Palette list rendering (Fleet-compact density).

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::Input;
use gpui_component::scroll::ScrollableElement;

use crate::theme::{TextRole, metrics, text, tokens};

use super::{CommandPalette, PaletteFrameKind, PaletteSelectNext, PaletteSelectPrev};

impl Render for CommandPalette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens();
        let m = metrics();
        let on_prev = cx.listener(|this, _: &PaletteSelectPrev, _, cx| {
            this.move_sel(-1);
            cx.notify();
        });
        let on_next = cx.listener(|this, _: &PaletteSelectNext, _, cx| {
            this.move_sel(1);
            cx.notify();
        });

        let frame_kind = self.stack.last().map(|f| f.kind);
        let empty = self.stack.last().is_none_or(|f| f.filtered_ix.is_empty());

        let mut rows = Vec::new();
        if let Some(frame) = self.stack.last() {
            for (row_ix, &data_ix) in frame.filtered_ix.iter().enumerate() {
                let Some(item) = frame.rows.get(data_ix) else {
                    continue;
                };
                let selected = frame.list_kb.is_row_focused(row_ix);
                let enabled = item.enabled;
                let title = item.title.clone();
                let detail = item.detail.clone();
                let trailing = item.trailing.clone();
                let entity = cx.entity().downgrade();
                rows.push(
                    div()
                        .id(SharedString::from(format!("palette-row-{row_ix}")))
                        .w_full()
                        .min_h(px(32.))
                        .px(m.tool_row_inset)
                        .py(m.space_xs)
                        .flex()
                        .flex_col()
                        .justify_center()
                        .gap(px(1.))
                        .rounded_md()
                        .hover(|s| s.bg(t.elevated_rgba()))
                        .when(selected, |d| d.bg(t.elevated_rgba()))
                        .when(!enabled, |d| d.opacity(0.45))
                        .cursor_pointer()
                        .on_click({
                            let entity = entity.clone();
                            move |_, _, cx| {
                                if let Some(view) = entity.upgrade() {
                                    view.update(cx, |this, cx| {
                                        if let Some(frame) = this.stack.last_mut() {
                                            frame
                                                .list_kb
                                                .set_cursor(frame.filtered_ix.len(), row_ix);
                                        }
                                        cx.notify();
                                    });
                                }
                            }
                        })
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(m.space_sm)
                                .child(
                                    text(TextRole::Label)
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(t.fg_rgba())
                                        .font_weight(if selected {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .child(title),
                                )
                                .when(!trailing.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .flex_shrink_0()
                                            .px(m.space_xs)
                                            .rounded_sm()
                                            .when(selected, |chip| chip.bg(t.surface_rgba()))
                                            .child(
                                                text(TextRole::Meta)
                                                    .font_family("monospace")
                                                    .text_color(t.muted_fg_rgba())
                                                    .child(trailing),
                                            ),
                                    )
                                }),
                        )
                        .when(!detail.is_empty(), |d| {
                            d.child(
                                text(TextRole::Meta)
                                    .w_full()
                                    .truncate()
                                    .text_color(t.muted_fg_rgba())
                                    .child(detail),
                            )
                        }),
                );
            }
        }

        let hint = match frame_kind {
            Some(PaletteFrameKind::Root) => crate::t!("palette.hint.root"),
            Some(PaletteFrameKind::Models) | Some(PaletteFrameKind::Thinking) => {
                crate::t!("palette.hint.submenu")
            }
            None => String::new(),
        };

        div()
            .id("command-palette")
            .track_focus(&self.focus_handle)
            .key_context("CommandPalette")
            .on_action(on_prev)
            .on_action(on_next)
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .child(
                div()
                    .id("palette-search")
                    .w_full()
                    .px(m.tool_row_inset)
                    .py(m.space_sm)
                    .border_b_1()
                    .border_color(t.border_rgba())
                    .child(Input::new(&self.filter_input).w_full()),
            )
            .child(
                div()
                    .id("palette-list")
                    .flex_1()
                    .min_h(px(0.))
                    .max_h(px(320.))
                    .px(m.space_xs)
                    .py(m.space_xs)
                    .overflow_y_scrollbar()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .when(empty, |d| {
                        d.child(
                            div()
                                .w_full()
                                .py(m.space_lg)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    text(TextRole::Meta)
                                        .text_color(t.muted_fg_rgba())
                                        .child(crate::t!("palette.empty")),
                                ),
                        )
                    })
                    .children(rows),
            )
            .when(!hint.is_empty(), |d| {
                d.child(
                    div()
                        .id("palette-hint")
                        .w_full()
                        .h(px(28.))
                        .px(m.tool_row_inset)
                        .flex()
                        .items_center()
                        .border_t_1()
                        .border_color(t.border_rgba())
                        .child(
                            text(TextRole::Meta)
                                .text_color(t.muted_fg_rgba())
                                .child(hint),
                        ),
                )
            })
    }
}
