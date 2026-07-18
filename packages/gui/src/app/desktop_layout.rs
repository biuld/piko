//! Layout helpers for [`super::desktop_app::DesktopApp`].

use gpui::*;

use piko_client_core::ClientIntent;

use crate::shell::{SessionPhaseView, derive_phase_view};
use crate::theme::{RoleAccent, island, metrics, tokens};
use crate::workbench::render::{
    render_activity_center, render_composer_panel, render_timeline_panel,
};
use crate::workbench::{ActivityItem, derive_activity, derive_composer, derive_timeline};

use super::desktop_app::{DesktopApp, JumpToLatest};
use super::workbench_chrome::render_pane_toggles;

pub(super) fn render_center(
    app: &DesktopApp,
    window: &mut Window,
    cx: &mut Context<DesktopApp>,
) -> AnyElement {
    let phase = derive_phase_view(app.bridge_state());
    let t = tokens();
    let m = metrics();
    match phase {
        SessionPhaseView::Live => render_live_center(app, window, cx).into_any_element(),
        SessionPhaseView::IdleNoSession => {
            render_pre_live_center(app, window, cx, "Select a session or type to start one.")
        }
        SessionPhaseView::Opening { .. } => {
            render_pre_live_center(app, window, cx, "Opening session…")
        }
        SessionPhaseView::Hydrating { .. } => {
            render_pre_live_center(app, window, cx, "Loading session…")
        }
        SessionPhaseView::Error { message } => island()
            .flex()
            .flex_col()
            .size_full()
            .child(render_pane_toggles(app, cx.entity().downgrade()))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(m.space_sm)
                    .child(
                        div()
                            .text_size(m.label_size)
                            .line_height(m.label_line_height)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.role_accent(RoleAccent::Danger))
                            .child("Error"),
                    )
                    .child(
                        div()
                            .text_size(m.body_size)
                            .line_height(m.body_line_height)
                            .text_color(t.muted_fg_rgba())
                            .child(message),
                    ),
            )
            .into_any_element(),
    }
}

fn render_pre_live_center(
    app: &DesktopApp,
    _window: &mut Window,
    cx: &mut Context<DesktopApp>,
    status: &'static str,
) -> AnyElement {
    let composer = derive_composer(app.bridge_state());
    let m = metrics();
    let entity = cx.entity().downgrade();
    div()
        .size_full()
        .flex()
        .flex_col()
        .gap(m.space_xs)
        .child(
            island()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .child(render_pane_toggles(app, entity.clone()))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(m.body_size)
                        .line_height(m.body_line_height)
                        .text_color(tokens().muted_fg_rgba())
                        .child(status),
                ),
        )
        .child(
            island()
                .flex_none()
                .py(m.space_sm)
                .child(center_workbench_surface(render_composer_panel(
                    &composer,
                    app.composer_input_entity(),
                    {
                        let entity = entity.clone();
                        move |_, window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| this.submit_composer(window, cx));
                            }
                        }
                    },
                    |_, _, _| {},
                    {
                        let entity = entity.clone();
                        move |_, _, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| this.cycle_model(cx));
                            }
                        }
                    },
                    move |_, _, cx| {
                        if let Some(view) = entity.upgrade() {
                            view.update(cx, |this, cx| this.cycle_thinking(cx));
                        }
                    },
                ))),
        )
        .into_any_element()
}

fn render_live_center(
    app: &DesktopApp,
    window: &mut Window,
    cx: &mut Context<DesktopApp>,
) -> impl IntoElement {
    let timeline = derive_timeline(app.bridge_state());
    let activity = derive_activity(app.bridge_state());
    let composer = derive_composer(app.bridge_state());
    let follow = app.follow_for_selected();
    let activity_expanded = app.activity_expanded();
    let entity = cx.entity().downgrade();
    div()
        .size_full()
        .h_full()
        .flex()
        .flex_col()
        .gap(metrics().space_xs)
        .overflow_hidden()
        .child(
            island()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .child(render_pane_toggles(app, entity.clone()))
                .child(render_timeline_panel(
                    &timeline,
                    follow,
                    app.timeline_scroll_handle(),
                    app.expanded_tools(),
                    {
                        let entity = entity.clone();
                        move |_, window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.action_jump_to_latest(&JumpToLatest, window, cx);
                                });
                            }
                        }
                    },
                    {
                        let entity = entity.clone();
                        move |row_id| {
                            let entity = entity.clone();
                            Box::new(move |_, _window, cx| {
                                if let Some(view) = entity.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.toggle_tool_detail(row_id.clone());
                                        cx.notify();
                                    });
                                }
                            })
                        }
                    },
                    window,
                    cx,
                )),
        )
        .child(
            island()
                .flex_none()
                .flex()
                .flex_col()
                .py(metrics().space_sm)
                .child(center_workbench_surface(render_activity_center(
                    &activity,
                    activity_expanded,
                    {
                        let entity = entity.clone();
                        move |_, _window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.toggle_activity_expanded();
                                    cx.notify();
                                });
                            }
                        }
                    },
                    {
                        let entity = entity.clone();
                        move |item: ActivityItem| {
                            let entity = entity.clone();
                            Box::new(move |_, window, cx| {
                                if let Some(view) = entity.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.handle_activity_item(item.clone(), window, cx);
                                    });
                                }
                            })
                        }
                    },
                )))
                .child(center_workbench_surface(render_composer_panel(
                    &composer,
                    app.composer_input_entity(),
                    {
                        let entity = entity.clone();
                        move |_, window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.submit_composer(window, cx);
                                });
                            }
                        }
                    },
                    {
                        let entity = entity.clone();
                        move |_, _window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.bridge_mut().intent(ClientIntent::CancelTurn);
                                    cx.notify();
                                });
                            }
                        }
                    },
                    {
                        let entity = entity.clone();
                        move |_, _window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| this.cycle_model(cx));
                            }
                        }
                    },
                    {
                        let entity = entity.clone();
                        move |_, _window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| this.cycle_thinking(cx));
                            }
                        }
                    },
                ))),
        )
}

fn center_workbench_surface(child: impl IntoElement) -> impl IntoElement {
    let m = metrics();
    div()
        .w_full()
        .px(m.space_sm)
        .flex()
        .justify_center()
        .child(div().w_full().max_w(m.reading_width).child(child))
}
