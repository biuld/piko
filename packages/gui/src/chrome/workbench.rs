//! Fixed Workbench layout: island tree, gutters, and resize interaction.
//!
//! Product layout (before visibility pruning):
//! ```text
//! Horizontal
//! ├─ Sessions
//! ├─ Vertical (center)
//! │  ├─ Timeline
//! │  └─ Composer
//! └─ Vertical (Agents ↕ Tree, only when either is visible)
//!    ├─ Agents
//!    └─ Tree
//! ```

use gpui::*;
use gpui_component::PixelsExt;
use gpui_component::resizable::{h_resizable, resizable_panel};

use crate::app::desktop_app::DesktopApp;
use crate::app::layout_state::{AGENTS_TREE_MIN_WIDTH, CENTER_MIN_WIDTH, SESSION_MIN_WIDTH};
use crate::chrome::center::render_center;
use crate::chrome::{
    IslandId, prune_island_tree, render_agents_tree_entities, workbench_island_tree,
};
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
        let show_agents_tree = show_agents || show_tree;

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

        if show_session && show_agents_tree {
            let agents_tree = self.render_agents_tree_stack(cx);
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
                                this.layout.agents_tree_width = sizes[2].as_f32();
                                this.persist_gui_config();
                            });
                        }
                    }
                })
                .child(
                    resizable_panel()
                        .size(px(self.layout.session_width))
                        .size_range(px(SESSION_MIN_WIDTH)..px(480.))
                        .child(island_slot(self.sessions.clone(), px(0.), gutter)),
                )
                .child(
                    resizable_panel()
                        .size_range(px(CENTER_MIN_WIDTH)..px(4000.))
                        .child(island_slot(center, px(0.), gutter)),
                )
                .child(
                    resizable_panel()
                        .size(px(self.layout.agents_tree_width))
                        .size_range(px(AGENTS_TREE_MIN_WIDTH)..px(520.))
                        .child(island_slot(agents_tree, px(0.), px(0.))),
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
                        .child(island_slot(self.sessions.clone(), px(0.), gutter)),
                )
                .child(
                    resizable_panel()
                        .size_range(px(CENTER_MIN_WIDTH)..px(4000.))
                        .child(island_slot(center, px(0.), px(0.))),
                )
                .into_any_element()
        } else if show_agents_tree {
            let agents_tree = self.render_agents_tree_stack(cx);
            h_resizable("workbench-center-agents-tree")
                .on_resize({
                    let entity = entity.clone();
                    move |state, _, cx| {
                        let sizes = state.read(cx).sizes().clone();
                        if let Some(size) = sizes.get(1)
                            && let Some(view) = entity.upgrade()
                        {
                            view.update(cx, |this, _| {
                                this.layout.agents_tree_width = size.as_f32();
                                this.persist_gui_config();
                            });
                        }
                    }
                })
                .child(
                    resizable_panel()
                        .size_range(px(CENTER_MIN_WIDTH)..px(4000.))
                        .child(island_slot(center, px(0.), gutter)),
                )
                .child(
                    resizable_panel()
                        .size(px(self.layout.agents_tree_width))
                        .size_range(px(AGENTS_TREE_MIN_WIDTH)..px(520.))
                        .child(island_slot(agents_tree, px(0.), px(0.))),
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

    fn render_agents_tree_stack(&self, cx: &Context<Self>) -> AnyElement {
        let live = self.bridge_state().is_live();
        let entity = cx.entity().downgrade();
        let agents_height = self.layout.agents_height;
        let show_agents = self.layout.is_docked_visible(IslandId::Agents, live);
        let show_tree = self.layout.is_docked_visible(IslandId::Tree, live);
        render_agents_tree_entities(
            self.agents.clone(),
            self.tree.clone(),
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

fn island_slot(
    child: impl IntoElement,
    leading: Pixels,
    trailing_gutter: Pixels,
) -> impl IntoElement {
    div()
        .size_full()
        .min_h(px(0.))
        .pl(leading)
        .pr(trailing_gutter)
        .overflow_hidden()
        .child(child)
}
