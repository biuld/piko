//! Pure flex solver backed by ratatui `Layout`.

use std::collections::HashMap;
use std::hash::Hash;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::flex::{Axis, Flex, FlexItem, Node};
use crate::modal::{ModalLayer, ModalPlacement};

/// Solved plane rects for one region id type.
#[derive(Clone, Debug)]
pub struct FramePlan<R> {
    pub rects: HashMap<R, Rect>,
    /// Optional stacked modal layers (low → high z).
    pub layers: Vec<LayerPlan<R>>,
}

/// One modal layer after solve.
#[derive(Clone, Debug)]
pub struct LayerPlan<R> {
    pub placement: ModalPlacement,
    pub rects: HashMap<R, Rect>,
}

impl<R: Eq + Hash> Default for FramePlan<R> {
    fn default() -> Self {
        Self {
            rects: HashMap::new(),
            layers: Vec::new(),
        }
    }
}

impl<R: Eq + Hash + Copy> FramePlan<R> {
    pub fn get(&self, region: R) -> Option<Rect> {
        self.rects.get(&region).copied()
    }

    /// Region-level z-hit: which region owns the cell, respecting layer order
    /// (top-most layer wins, then the plane).
    ///
    /// Ratatui cell semantics: `x in [rect.x, rect.x + rect.width)`.
    pub fn hit_test(&self, x: u16, y: u16) -> Option<(R, Option<usize>)> {
        for (i, layer) in self.layers.iter().enumerate().rev() {
            for (region, rect) in &layer.rects {
                if rect_contains(*rect, x, y) {
                    return Some((*region, Some(i)));
                }
            }
        }
        for (region, rect) in &self.rects {
            if rect_contains(*rect, x, y) {
                return Some((*region, None));
            }
        }
        None
    }
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

/// Solve a plane tree in `area` (no modals).
///
/// `area` is the **viewport root** for [`FlexSize::Vw`] / [`FlexSize::Vh`].
pub fn solve_flex<R: Copy + Eq + Hash>(area: Rect, root: &Node<R>) -> HashMap<R, Rect> {
    let mut rects = HashMap::new();
    solve_node(area, area, root, &mut rects);
    rects
}

/// Solve plane + modal layers into a full frame plan.
///
/// Modal roots are resolved against `area` according to [`ModalPlacement`].
/// Plane `Vw`/`Vh` use the body `area` as viewport root; each modal layer uses
/// its placement **host** rect as that layer's viewport root.
pub fn solve<R: Copy + Eq + Hash>(
    area: Rect,
    plane: &Node<R>,
    modals: &[ModalLayer<R>],
) -> FramePlan<R> {
    let mut plan = FramePlan {
        rects: solve_flex(area, plane),
        layers: Vec::with_capacity(modals.len()),
    };
    for modal in modals {
        let host = placement_host(area, modal.placement, modal.host_band_height);
        let rects = solve_flex(host, &modal.tree);
        plan.layers.push(LayerPlan {
            placement: modal.placement,
            rects,
        });
    }
    plan
}

fn placement_host(body: Rect, placement: ModalPlacement, band_height: u16) -> Rect {
    match placement {
        ModalPlacement::CoverBody => body,
        ModalPlacement::ComposerBand => {
            let h = band_height.min(body.height);
            Rect {
                x: body.x,
                y: body.y.saturating_add(body.height.saturating_sub(h)),
                width: body.width,
                height: h,
            }
        }
        ModalPlacement::Centered {
            max_width,
            max_height,
        } => {
            let w = max_width.min(body.width);
            let h = max_height.min(body.height);
            let x = body.x.saturating_add(body.width.saturating_sub(w) / 2);
            let y = body.y.saturating_add(body.height.saturating_sub(h) / 2);
            Rect {
                x,
                y,
                width: w,
                height: h,
            }
        }
    }
}

fn solve_node<R: Copy + Eq + Hash>(
    viewport: Rect,
    area: Rect,
    node: &Node<R>,
    rects: &mut HashMap<R, Rect>,
) {
    match node {
        Node::Leaf(region) => {
            rects.insert(*region, area);
        }
        Node::Flex(flex) => solve_flex_container(viewport, area, flex, rects),
    }
}

fn solve_flex_container<R: Copy + Eq + Hash>(
    viewport: Rect,
    area: Rect,
    flex: &Flex<R>,
    rects: &mut HashMap<R, Rect>,
) {
    if flex.children.is_empty() {
        return;
    }
    let parent_main = match flex.direction {
        Axis::Column => area.height,
        Axis::Row => area.width,
    };
    let constraints: Vec<Constraint> = flex
        .children
        .iter()
        .map(|c: &FlexItem<R>| c.size.to_constraint(viewport, parent_main))
        .collect();
    let direction = match flex.direction {
        Axis::Column => Direction::Vertical,
        Axis::Row => Direction::Horizontal,
    };
    let chunks = Layout::default()
        .direction(direction)
        .constraints(constraints)
        .split(area);
    for (child, chunk) in flex.children.iter().zip(chunks.iter()) {
        solve_node(viewport, *chunk, &child.child, rects);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flex::{FlexItem, FlexSize, flex_column, flex_row, leaf};

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
    enum R {
        A,
        B,
        C,
    }

    #[test]
    fn column_grow_and_fixed() {
        let tree = flex_column(vec![
            FlexItem::new(FlexSize::Grow { weight: 1, min: 0 }, leaf(R::A)),
            FlexItem::fixed(1, leaf(R::B)),
        ]);
        let plan = solve_flex(Rect::new(0, 0, 80, 24), &tree);
        assert_eq!(plan.get(&R::A), Some(&Rect::new(0, 0, 80, 23)));
        assert_eq!(plan.get(&R::B), Some(&Rect::new(0, 23, 80, 1)));
    }

    #[test]
    fn row_splits() {
        let tree = flex_row(vec![
            FlexItem::fixed(20, leaf(R::A)),
            FlexItem::grow(1, leaf(R::B)),
        ]);
        let plan = solve_flex(Rect::new(0, 0, 80, 10), &tree);
        assert_eq!(plan.get(&R::A).map(|r| r.width), Some(20));
        assert_eq!(plan.get(&R::B).map(|r| r.width), Some(60));
    }

    #[test]
    fn modal_cover_body_layer() {
        let plane = flex_column(vec![FlexItem::grow(1, leaf(R::A))]);
        let modals = [ModalLayer {
            placement: ModalPlacement::CoverBody,
            host_band_height: 0,
            tree: flex_column(vec![FlexItem::grow(1, leaf(R::C))]),
        }];
        let frame = solve(Rect::new(0, 0, 40, 20), &plane, &modals);
        assert!(frame.rects.contains_key(&R::A));
        assert_eq!(frame.layers.len(), 1);
        assert_eq!(
            frame.layers[0].rects.get(&R::C),
            Some(&Rect::new(0, 0, 40, 20))
        );
    }

    #[test]
    fn parent_percent_column() {
        let tree = flex_column(vec![
            FlexItem::percent(25, leaf(R::A)),
            FlexItem::grow(1, leaf(R::B)),
        ]);
        let plan = solve_flex(Rect::new(0, 0, 80, 40), &tree);
        assert_eq!(plan.get(&R::A).map(|r| r.height), Some(10)); // 25% of 40
        assert_eq!(plan.get(&R::B).map(|r| r.height), Some(30));
    }

    #[test]
    fn parent_percent_row() {
        let tree = flex_row(vec![
            FlexItem::percent(30, leaf(R::A)),
            FlexItem::grow(1, leaf(R::B)),
        ]);
        let plan = solve_flex(Rect::new(0, 0, 100, 10), &tree);
        assert_eq!(plan.get(&R::A).map(|r| r.width), Some(30));
        assert_eq!(plan.get(&R::B).map(|r| r.width), Some(70));
    }

    #[test]
    fn vh_uses_root_height_even_when_nested() {
        // Nested column: outer takes full root; inner vh(50) → half of root h, not half of a shrink.
        let inner = flex_column(vec![
            FlexItem::vh(50, leaf(R::A)),
            FlexItem::grow(1, leaf(R::B)),
        ]);
        let tree = flex_column(vec![FlexItem::grow(1, inner)]);
        let plan = solve_flex(Rect::new(0, 0, 80, 40), &tree);
        assert_eq!(plan.get(&R::A).map(|r| r.height), Some(20)); // 50% of root 40
    }

    #[test]
    fn vw_uses_root_width() {
        let tree = flex_row(vec![
            FlexItem::vw(25, leaf(R::A)),
            FlexItem::grow(1, leaf(R::B)),
        ]);
        let plan = solve_flex(Rect::new(0, 0, 80, 10), &tree);
        assert_eq!(plan.get(&R::A).map(|r| r.width), Some(20)); // 25% of root 80
        assert_eq!(plan.get(&R::B).map(|r| r.width), Some(60));
    }
}
