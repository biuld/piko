//! Fixed Workbench layout: island tree, gutters, and resize interaction.
//!
//! Product layout (before visibility pruning):
//! ```text
//! Horizontal
//! ├─ Sessions (left)
//! ├─ Vertical (center)
//! │  ├─ Timeline
//! │  └─ Composer
//! └─ Vertical (right: Agents ↕ Tree, only when either is visible)
//!    ├─ Agents
//!    └─ Tree
//! ```

use gpui::*;
use gpui_component::PixelsExt;
use gpui_component::resizable::{h_resizable, resizable_panel};

use crate::app::desktop_app::DesktopApp;
use crate::app::layout_state::{CENTER_MIN_WIDTH, RIGHT_COLUMN_MIN_WIDTH, SESSION_MIN_WIDTH};
use crate::shell::workbench::center::render_center;
use crate::shell::workbench::left::{column_slot, render_left_column};
use crate::shell::workbench::right::render_right_column;
use crate::shell::workbench::{IslandId, prune_island_tree, workbench_island_tree};
use crate::theme::metrics;

impl DesktopApp {
    pub(crate) fn render_workbench_row(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let live = self.bridge_state().is_live();
        let entity = cx.entity().downgrade();
        let m = metrics();
        let gutter = m.island_gutter;

        let show_session = self.layout.is_docked_visible(IslandId::Sessions, live);
        let show_agents = self.layout.is_docked_visible(IslandId::Agents, live);
        let show_tree = self.layout.is_docked_visible(IslandId::Tree, live);
        let show_right = show_agents || show_tree;

        let _pruned = prune_island_tree(&workbench_island_tree(), &|id| {
            self.layout.is_docked_visible(id, live)
        });

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

        if show_session && show_right {
            let right = self.render_right_column_stack(cx);
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
                                this.layout.right_column_width = sizes[2].as_f32();
                                this.persist_gui_config();
                            });
                        }
                    }
                })
                .child(
                    resizable_panel()
                        .size(px(self.layout.session_width))
                        .size_range(px(SESSION_MIN_WIDTH)..px(480.))
                        .child(render_left_column(self.sessions.clone(), gutter)),
                )
                .child(
                    resizable_panel()
                        .size_range(px(CENTER_MIN_WIDTH)..px(4000.))
                        .child(column_slot(center, px(0.), gutter)),
                )
                .child(
                    resizable_panel()
                        .size(px(self.layout.right_column_width))
                        .size_range(px(RIGHT_COLUMN_MIN_WIDTH)..px(520.))
                        .child(column_slot(right, px(0.), px(0.))),
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
                        .child(render_left_column(self.sessions.clone(), gutter)),
                )
                .child(
                    resizable_panel()
                        .size_range(px(CENTER_MIN_WIDTH)..px(4000.))
                        .child(column_slot(center, px(0.), px(0.))),
                )
                .into_any_element()
        } else if show_right {
            let right = self.render_right_column_stack(cx);
            h_resizable("workbench-center-right")
                .on_resize({
                    let entity = entity.clone();
                    move |state, _, cx| {
                        let sizes = state.read(cx).sizes().clone();
                        if let Some(size) = sizes.get(1)
                            && let Some(view) = entity.upgrade()
                        {
                            view.update(cx, |this, _| {
                                this.layout.right_column_width = size.as_f32();
                                this.persist_gui_config();
                            });
                        }
                    }
                })
                .child(
                    resizable_panel()
                        .size_range(px(CENTER_MIN_WIDTH)..px(4000.))
                        .child(column_slot(center, px(0.), gutter)),
                )
                .child(
                    resizable_panel()
                        .size(px(self.layout.right_column_width))
                        .size_range(px(RIGHT_COLUMN_MIN_WIDTH)..px(520.))
                        .child(column_slot(right, px(0.), px(0.))),
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

    fn render_right_column_stack(&self, cx: &Context<Self>) -> AnyElement {
        let live = self.bridge_state().is_live();
        let entity = cx.entity().downgrade();
        let agents_height = self.layout.agents_height;
        let show_agents = self.layout.is_docked_visible(IslandId::Agents, live);
        let show_tree = self.layout.is_docked_visible(IslandId::Tree, live);
        render_right_column(
            self.agents.clone().into_any_element(),
            self.tree.clone().into_any_element(),
            show_agents,
            show_tree,
            agents_height,
            Box::new({
                let entity = entity.clone();
                move |height, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, _| {
                            this.layout.agents_height = height;
                            this.persist_gui_config();
                        });
                    }
                }
            }),
        )
        .into_any_element()
    }
}
