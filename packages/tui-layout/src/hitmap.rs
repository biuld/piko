//! Derived hit-testing over solved layout (product-agnostic).
//!
//! Two tiers collapse into one flat hit map per frame:
//!
//! - **Plane / layer rects** (from [`FramePlan`]) give each region a `z`
//!   (plane = 0, layer `i` = `i + 1`).
//! - **Surface panels** stamp their [`Component`] sub-regions with the region
//!   id and add a surface-default entry (`element: None`) over the whole host
//!   rect, so clicks on empty modal space never fall through to lower layers.

use std::hash::Hash;

use ratatui::{Frame, layout::Rect};

use crate::engine::FramePlan;
use crate::interaction::InteractionState;

/// Region-stamped interactive region produced by a [`SurfacePanel`].
///
/// `element: None` is the surface default action (background / non-interactive
/// cells inside the host rect).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct HitRegion<R, E> {
    pub region: R,
    pub rect: Rect,
    pub element: Option<E>,
}

/// Unified component base: paint + own interactive regions.
///
/// The Rust analog of a component parent class. `E` is the product element id
/// (what an interaction targets), `C` the product render context (theme etc.).
/// The trait is a shared vocabulary — composition is flat enumeration, never a
/// recursive point-query.
pub trait Component<E, C: ?Sized> {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &C);

    /// Paint with component-scoped interaction state. Non-interactive
    /// components inherit the stateless paint path.
    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &C,
        _interaction: InteractionState<E>,
    ) {
        self.render(frame, area, ctx);
    }

    /// Interactive sub-regions inside `area`, using the same geometry as
    /// [`Self::render`]. `Vec::new()` = not interactive.
    fn component_regions(&self, area: Rect) -> Vec<(Rect, E)>;

    /// Optional now; enables pointer-focus routing on the same base later.
    fn focusable(&self) -> bool {
        false
    }
}

/// Region-level wrapper over [`Component`]: adds the region id that components
/// deliberately do not know, plus the surface-default entry.
pub trait SurfacePanel<R: Copy, E, C: ?Sized>: Component<E, C> {
    /// This panel's region id (its slot in the plane or a modal layer).
    fn region(&self) -> R;

    /// Component regions stamped with this panel's region id, plus one
    /// `element: None` entry over the whole area (surface default action).
    fn hit_regions(&self, area: Rect) -> Vec<HitRegion<R, E>> {
        let id = self.region();
        let mut out: Vec<_> = self
            .component_regions(area)
            .into_iter()
            .map(|(rect, element)| HitRegion {
                region: id,
                rect,
                element: Some(element),
            })
            .collect();
        out.push(HitRegion {
            region: id,
            rect: area,
            element: None,
        });
        out
    }
}

/// One resolved entry in the flat hit map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hit<R, E> {
    pub region: R,
    pub element: Option<E>,
    pub rect: Rect,
    /// Derived: plane = 0, layer `i` = `i + 1`.
    pub z: u16,
    /// `None` = plane; `Some(i)` = layer index.
    pub layer: Option<usize>,
}

impl<R: Copy, E: Copy> Hit<R, E> {
    /// Ratatui cell semantics: `x in [rect.x, rect.x + rect.width)`.
    pub fn contains(&self, x: u16, y: u16) -> bool {
        let r = self.rect;
        x >= r.x && x < r.x.saturating_add(r.width) && y >= r.y && y < r.y.saturating_add(r.height)
    }
}

/// Flat per-frame hit map. Built once per frame; serves many pointer events.
#[derive(Clone, Debug, Default)]
pub struct HitMap<R, E> {
    pub hits: Vec<Hit<R, E>>,
}

impl<R: Copy + Eq + Hash, E: Copy + Eq + Hash> HitMap<R, E> {
    /// Top-most region owning the cell; within one `z`, an element beats its
    /// surface-default entry. `None` = no region owns the coordinate.
    pub fn hit_test(&self, x: u16, y: u16) -> Option<&Hit<R, E>> {
        self.hits
            .iter()
            .filter(|h| h.contains(x, y))
            .max_by_key(|h| (h.z, h.element.is_some()))
    }

    /// Highest modal layer represented in this map.
    pub fn top_layer(&self) -> Option<usize> {
        self.hits.iter().filter_map(|hit| hit.layer).max()
    }

    /// Whether `hit` belongs to the highest modal layer in this map.
    pub fn is_top_layer_hit(&self, hit: Option<&Hit<R, E>>) -> bool {
        let top_layer = self.top_layer();
        top_layer.is_some() && hit.and_then(|entry| entry.layer) == top_layer
    }

    /// Resolve the coordinate only when its top-most owner is on the highest
    /// modal layer. A lower-layer result is treated as outside the top modal.
    pub fn hit_test_top_layer(&self, x: u16, y: u16) -> Option<&Hit<R, E>> {
        let hit = self.hit_test(x, y);
        self.is_top_layer_hit(hit).then_some(hit).flatten()
    }
}

/// Build the unified hit map from a solved plan.
///
/// `regions` maps each painted region + its rect to region-stamped hit
/// regions (typically a [`SurfacePanel::hit_regions`] call). Entries are
/// z-ordered by construction: plane first, then layers low → high.
pub fn build_hitmap<R, E, F>(plan: &FramePlan<R>, mut regions: F) -> HitMap<R, E>
where
    R: Copy + Eq + Hash,
    E: Copy + Eq + Hash,
    F: FnMut(R, Rect) -> Vec<HitRegion<R, E>>,
{
    let mut hits = Vec::new();
    for (region, rect) in &plan.rects {
        for hr in regions(*region, *rect) {
            hits.push(Hit {
                region: hr.region,
                element: hr.element,
                rect: hr.rect,
                z: 0,
                layer: None,
            });
        }
    }
    for (i, layer) in plan.layers.iter().enumerate() {
        let z = (i + 1) as u16;
        for (region, rect) in &layer.rects {
            for hr in regions(*region, *rect) {
                hits.push(Hit {
                    region: hr.region,
                    element: hr.element,
                    rect: hr.rect,
                    z,
                    layer: Some(i),
                });
            }
        }
    }
    HitMap { hits }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        flex::{FlexItem, flex_column, leaf},
        modal::{ModalLayer, ModalPlacement},
        solve,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
    enum R {
        Plane,
        Modal,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
    enum E {
        Row(usize),
    }

    fn panel_regions(_r: R, rect: Rect) -> Vec<HitRegion<R, E>> {
        // Component row at the top, then the surface default over the whole area.
        vec![
            HitRegion {
                region: _r,
                rect: Rect::new(rect.x, rect.y, rect.width, 1),
                element: Some(E::Row(0)),
            },
            HitRegion {
                region: _r,
                rect,
                element: None,
            },
        ]
    }

    fn plan() -> FramePlan<R> {
        let plane = flex_column(vec![FlexItem::grow(1, leaf(R::Plane))]);
        let modals = vec![ModalLayer {
            placement: ModalPlacement::Centered {
                max_width: 40,
                max_height: 20,
            },
            host_band_height: 0,
            tree: flex_column(vec![FlexItem::grow(1, leaf(R::Modal))]),
        }];
        solve(Rect::new(0, 0, 80, 40), &plane, &modals)
    }

    #[test]
    fn modal_layer_wins_over_plane() {
        let plan = plan();
        let map = build_hitmap(&plan, panel_regions);
        // Centered host: x 20..60, y 10..30. The row occupies the top row.
        let hit = map.hit_test(40, 10).expect("centered modal cell");
        assert_eq!(hit.region, R::Modal);
        assert_eq!(hit.z, 1);
        assert_eq!(hit.layer, Some(0));
        // The row entry and the surface default overlap; element wins at same z.
        assert_eq!(hit.element, Some(E::Row(0)));
        assert_eq!(map.top_layer(), Some(0));
        assert!(map.is_top_layer_hit(Some(hit)));
        assert_eq!(map.hit_test_top_layer(40, 10), Some(hit));
    }

    #[test]
    fn surface_default_does_not_fall_through() {
        let plan = plan();
        let map = build_hitmap(&plan, panel_regions);
        // Modal host below the single row: still the modal's surface-default
        // entry, never the plane.
        let hit = map.hit_test(40, 25).expect("inside modal host");
        assert_eq!(hit.region, R::Modal);
        assert_eq!(hit.element, None);
    }

    #[test]
    fn plane_is_z_zero_fallback() {
        let plan = plan();
        let map = build_hitmap(&plan, panel_regions);
        // Outside the centered host (body corner): plane owns it.
        let hit = map.hit_test(1, 1).expect("plane cell");
        assert_eq!(hit.region, R::Plane);
        assert_eq!(hit.z, 0);
        assert_eq!(hit.layer, None);
        assert!(!map.is_top_layer_hit(Some(hit)));
        assert_eq!(map.hit_test_top_layer(1, 1), None);
    }

    #[test]
    fn edge_coordinates_are_inclusive_on_left_side() {
        let plan = plan();
        let map = build_hitmap(&plan, panel_regions);
        // x == host.x is inside; x == host.x + host.width is outside.
        let host = plan.layers[0].rects.get(&R::Modal).copied().unwrap();
        assert!(map.hit_test(host.x, host.y).is_some());
        assert!(
            map.hit_test(host.x.saturating_add(host.width), host.y)
                .is_none_or(|h| h.region != R::Modal)
        );
    }

    #[test]
    fn frame_plan_region_hit_test_respects_layers() {
        let plan = plan();
        let (region, layer) = plan.hit_test(40, 20).expect("layer-owned cell");
        assert_eq!(region, R::Modal);
        assert_eq!(layer, Some(0));
        let (region, _) = plan.hit_test(1, 1).unwrap();
        assert_eq!(region, R::Plane);
    }
}
