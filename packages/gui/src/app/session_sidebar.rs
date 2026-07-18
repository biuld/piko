//! Session sidebar rendering for DesktopApp.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;

use crate::shell::{SessionRowKind, derive_sidebar};
use crate::theme::{island, metrics, tokens};
use piko_client_core::ClientState;

use super::desktop_app::{DesktopApp, NewSession};

pub(crate) fn render_session_sidebar(
    state: &ClientState,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let sidebar_vm = derive_sidebar(state);
    let m = metrics();

    island()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .h(m.panel_header_height)
                .px(m.space_md)
                .flex()
                .items_center()
                .gap(m.space_sm)
                .child(
                    div()
                        .text_size(m.label_size)
                        .line_height(m.label_line_height)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Sessions"),
                )
                .child(div().flex_1())
                .child(
                    Button::new("new-session")
                        .label("+")
                        .tooltip("New Session")
                        .ghost()
                        .small()
                        .compact()
                        .on_click({
                            let entity = entity.clone();
                            move |_, window, cx| {
                                if let Some(view) = entity.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.action_new_session(&NewSession, window, cx);
                                    });
                                }
                            }
                        }),
                ),
        )
        .child(
            div()
                .flex_1()
                .overflow_y_scrollbar()
                .px(m.space_xs)
                .pb(m.space_xs)
                .children(sidebar_vm.rows.iter().enumerate().map(|(ix, row)| {
                    let entity = entity.clone();
                    let session_id = row.session_id.clone();
                    let is_live = row.kind == SessionRowKind::LiveTarget;
                    let is_pending = row.kind == SessionRowKind::PendingTarget;
                    let t = tokens();

                    div()
                        .id(SharedString::from(format!("session-row-{ix}")))
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
                        .on_click(move |_, _window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.handle_open_session(session_id.clone(), cx);
                                });
                            }
                        })
                })),
        )
}
