//! Fixed Agents ↕ Tree vertical split (part of the Workbench layout tree).
//!
//! This is not a layout unit. Agents and Tree remain separate
//! islands; chrome only stacks them when both are visible.

use gpui::*;
use gpui_component::PixelsExt;
use gpui_component::resizable::{resizable_panel, v_resizable};

use crate::islands::{AgentsIsland, TreeIsland};
use crate::theme::metrics;

type AgentsHeightHandler = Box<dyn Fn(f32, &mut App) + 'static>;

/// Mount Agents / Tree Entity islands in the dock or sheet split.
pub fn render_agents_tree_entities(
    agents: Entity<AgentsIsland>,
    tree: Entity<TreeIsland>,
    show_agents: bool,
    show_tree: bool,
    agents_height: f32,
    on_agents_height: AgentsHeightHandler,
) -> impl IntoElement {
    let m = metrics();
    let gutter = m.island_gutter;

    div()
        .id("agents-tree-split")
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(match (show_agents, show_tree) {
            (true, true) => div()
                .size_full()
                .child(
                    v_resizable("agents-tree-v")
                        .on_resize(move |state, _, cx| {
                            let sizes = state.read(cx).sizes();
                            if let Some(size) = sizes.first() {
                                on_agents_height(size.as_f32(), cx);
                            }
                        })
                        .child(
                            resizable_panel()
                                .size(px(agents_height))
                                .size_range(px(160.)..px(2000.))
                                .child(vertical_island_slot(agents, px(0.), gutter)),
                        )
                        .child(
                            resizable_panel()
                                .size_range(px(180.)..px(2000.))
                                .child(vertical_island_slot(tree, px(0.), px(0.))),
                        ),
                )
                .into_any_element(),
            (true, false) => vertical_island_slot(agents, px(0.), px(0.)).into_any_element(),
            (false, true) => vertical_island_slot(tree, px(0.), px(0.)).into_any_element(),
            (false, false) => div().into_any_element(),
        })
}

fn vertical_island_slot(
    child: impl IntoElement,
    top_padding: Pixels,
    bottom_padding: Pixels,
) -> impl IntoElement {
    div()
        .size_full()
        .pt(top_padding)
        .pb(bottom_padding)
        .child(child)
}
