//! Settings body: horizontal Nav | Panel island workspaces.

use std::collections::HashMap;

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::theme::metrics;
use piko_chrome::{IslandAxis, IslandNode};

/// Realize the declared Settings workspace tree. Product widths stay here;
/// membership, order, and split direction come from [`IslandNode`].
pub fn body_slots<Id>(
    tree: &IslandNode<Id>,
    nav_id: Id,
    panel_id: Id,
    nav: impl IntoElement,
    panel: impl IntoElement,
) -> impl IntoElement
where
    Id: Copy + Eq + std::hash::Hash,
{
    let m = metrics();
    let mut slots = HashMap::from([
        (nav_id, nav.into_any_element()),
        (panel_id, panel.into_any_element()),
    ]);

    div()
        .id("settings-body")
        .flex_1()
        .min_h(px(0.))
        .overflow_hidden()
        .p(m.island_gutter)
        .child(render_workspace_node(tree, nav_id, &mut slots))
}

fn render_workspace_node<Id>(
    node: &IslandNode<Id>,
    nav_id: Id,
    slots: &mut HashMap<Id, AnyElement>,
) -> AnyElement
where
    Id: Copy + Eq + std::hash::Hash,
{
    let m = metrics();
    match node {
        IslandNode::Island(id) => {
            let child = slots
                .remove(id)
                .expect("settings workspace leaf must have one registered slot");
            if *id == nav_id {
                div()
                    .id("settings-nav-slot")
                    .w(px(220.))
                    .flex_shrink_0()
                    .h_full()
                    .min_h(px(0.))
                    .child(child)
                    .into_any_element()
            } else {
                div()
                    .id("settings-panel-slot")
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .min_h(px(0.))
                    .child(child)
                    .into_any_element()
            }
        }
        IslandNode::Split { axis, children } => div()
            .size_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .flex()
            .when(*axis == IslandAxis::Horizontal, |d| d.flex_row())
            .when(*axis == IslandAxis::Vertical, |d| d.flex_col())
            .gap(m.island_gutter)
            .children(
                children
                    .iter()
                    .map(|child| render_workspace_node(child, nav_id, slots)),
            )
            .into_any_element(),
    }
}
