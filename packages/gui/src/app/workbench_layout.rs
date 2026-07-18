//! Responsive outer Workbench layout and persisted split sizes.

use gpui::*;
use gpui_component::PixelsExt;
use gpui_component::resizable::{h_resizable, resizable_panel};

use crate::inspector::{derive_conversation_map, render_inspector_panel};
use crate::theme::metrics;
use crate::workbench::derive_agent_tree;

use super::desktop_app::DesktopApp;
use super::desktop_layout::render_center;
use super::layout_state::{CENTER_MIN_WIDTH, INSPECTOR_MIN_WIDTH, SESSION_MIN_WIDTH};
use super::session_sidebar::render_session_sidebar;
use super::workbench_chrome::inspector_handlers;

impl DesktopApp {
    pub(crate) fn render_workbench_row(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let show_session = self.layout.show_session_pane();
        let live = self.bridge_state().is_live();
        let show_inspector = self.layout.show_inspector_pane(live);
        let entity = cx.entity().downgrade();

        let m = metrics();
        let center = div()
            .size_full()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(render_center(self, window, cx)),
            );

        if show_session && show_inspector {
            let inspector = self.render_inspector_element(cx);
            h_resizable("workbench-h")
                .on_resize({
                    let entity = entity.clone();
                    move |state, _, cx| {
                        let sizes = state.read(cx).sizes().clone();
                        if sizes.len() == 3
                            && let Some(view) = entity.upgrade()
                        {
                            view.update(cx, |this, _| {
                                this.layout.session_width = sizes[0].as_f32();
                                this.layout.inspector_width = sizes[2].as_f32();
                                this.persist_gui_config();
                            });
                        }
                    }
                })
                .child(
                    resizable_panel()
                        .size(px(self.layout.session_width))
                        .size_range(px(SESSION_MIN_WIDTH)..px(480.))
                        .child(island_slot(
                            render_session_sidebar(self.bridge_state(), entity),
                            px(0.),
                            m.space_xs,
                        )),
                )
                .child(
                    resizable_panel()
                        .size_range(px(CENTER_MIN_WIDTH)..px(4000.))
                        .child(island_slot(center, m.space_xs, m.space_xs)),
                )
                .child(
                    resizable_panel()
                        .size(px(self.layout.inspector_width))
                        .size_range(px(INSPECTOR_MIN_WIDTH)..px(520.))
                        .child(island_slot(inspector, m.space_xs, px(0.))),
                )
                .into_any_element()
        } else if show_session {
            h_resizable("workbench-session-center")
                .on_resize({
                    let entity = entity.clone();
                    move |state, _, cx| {
                        let sizes = state.read(cx).sizes().clone();
                        if let Some(size) = sizes.first()
                            && let Some(view) = entity.upgrade()
                        {
                            view.update(cx, |this, _| {
                                this.layout.session_width = size.as_f32();
                                this.persist_gui_config();
                            });
                        }
                    }
                })
                .child(
                    resizable_panel()
                        .size(px(self.layout.session_width))
                        .size_range(px(SESSION_MIN_WIDTH)..px(480.))
                        .child(island_slot(
                            render_session_sidebar(self.bridge_state(), entity),
                            px(0.),
                            m.space_xs,
                        )),
                )
                .child(
                    resizable_panel()
                        .size_range(px(CENTER_MIN_WIDTH)..px(4000.))
                        .child(island_slot(center, m.space_xs, px(0.))),
                )
                .into_any_element()
        } else if show_inspector {
            let inspector = self.render_inspector_element(cx);
            h_resizable("workbench-center-inspector")
                .on_resize({
                    let entity = entity.clone();
                    move |state, _, cx| {
                        let sizes = state.read(cx).sizes().clone();
                        if let Some(size) = sizes.get(1)
                            && let Some(view) = entity.upgrade()
                        {
                            view.update(cx, |this, _| {
                                this.layout.inspector_width = size.as_f32();
                                this.persist_gui_config();
                            });
                        }
                    }
                })
                .child(
                    resizable_panel()
                        .size_range(px(CENTER_MIN_WIDTH)..px(4000.))
                        .child(island_slot(center, px(0.), m.space_xs)),
                )
                .child(
                    resizable_panel()
                        .size(px(self.layout.inspector_width))
                        .size_range(px(INSPECTOR_MIN_WIDTH)..px(520.))
                        .child(island_slot(inspector, m.space_xs, px(0.))),
                )
                .into_any_element()
        } else {
            div()
                .size_full()
                .min_h(px(0.))
                .overflow_hidden()
                .flex()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(CENTER_MIN_WIDTH))
                        .min_h(px(0.))
                        .overflow_hidden()
                        .child(center),
                )
                .into_any_element()
        }
    }

    fn render_inspector_element(&self, cx: &Context<Self>) -> AnyElement {
        let agent_tree = derive_agent_tree(self.bridge_state());
        let expanded = self.map_expanded_for_selected();
        let map = derive_conversation_map(self.bridge_state(), self.map_preview_id(), &expanded);
        let entity = cx.entity().downgrade();
        let width = self.layout.inspector_width;
        render_inspector_panel(&agent_tree, &map, width, inspector_handlers(entity))
            .into_any_element()
    }
}

fn island_slot(
    child: impl IntoElement,
    left_padding: Pixels,
    right_padding: Pixels,
) -> impl IntoElement {
    div()
        .size_full()
        .min_h(px(0.))
        .pl(left_padding)
        .pr(right_padding)
        .overflow_hidden()
        .child(child)
}
