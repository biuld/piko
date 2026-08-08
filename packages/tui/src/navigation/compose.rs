//! Product layout trees with `piko-tui-layout` primitives only.
//!
//! Workspace plane = Stream + optional Notice/Suggest + Composer.
//! Surfaces = modal z-stack (Browse / Select / Decide).

use piko_tui_layout::{FlexItem, ModalLayer, Node, cells_from_percent, flex_column, leaf};
use ratatui::layout::Rect;

use super::{Region, SurfaceId, SurfaceIntent};

/// Per-frame sizes from feature state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaneMetrics {
    pub notice: bool,
    pub suggest: bool,
    pub suggestion_count: usize,
    pub composer_height: u16,
    pub body_height: u16,
}

/// Stable workspace plane (always composed).
pub fn compose_plane(m: PlaneMetrics) -> Node<Region> {
    let mut children = vec![FlexItem::grow(1, leaf(Region::Stream))];
    if m.notice {
        children.push(FlexItem::fixed(1, leaf(Region::Notice)));
    }
    if m.suggest {
        children.push(FlexItem::fixed(
            suggestion_height(m.suggestion_count),
            leaf(Region::Suggest),
        ));
    }
    children.push(FlexItem::fixed(m.composer_height, leaf(Region::Composer)));
    flex_column(children)
}

/// Host-priority + focus → modal layers (at most one product surface for now).
pub fn compose_modals(
    host_surface: Option<SurfaceId>,
    metrics: PlaneMetrics,
    body: Rect,
) -> Vec<ModalLayer<Region>> {
    let Some(surface) = host_surface else {
        return Vec::new();
    };
    let band = select_band_height(surface, metrics);
    vec![surface.modal_layer(body, band)]
}

fn select_band_height(surface: SurfaceId, m: PlaneMetrics) -> u16 {
    if surface.intent() != SurfaceIntent::Select {
        return 0;
    }
    let target = m.composer_height.max(cells_from_percent(m.body_height, 40));
    let max = m.body_height.saturating_sub(4);
    target.min(max).max(m.composer_height.min(m.body_height))
}

fn suggestion_height(count: usize) -> u16 {
    (count as u16 + 2).min(8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_tui_layout::{ModalPlacement, solve, solve_flex};

    fn metrics() -> PlaneMetrics {
        PlaneMetrics {
            notice: false,
            suggest: false,
            suggestion_count: 0,
            composer_height: 5,
            body_height: 30,
        }
    }

    #[test]
    fn plane_is_stream_and_composer() {
        let rects = solve_flex(Rect::new(0, 0, 80, 30), &compose_plane(metrics()));
        assert!(rects.contains_key(&Region::Stream));
        assert!(rects.contains_key(&Region::Composer));
        assert!(!rects.keys().any(|r| matches!(r, Region::Surface(_))));
        assert_eq!(rects.get(&Region::Stream).map(|r| r.height), Some(25));
    }

    #[test]
    fn browse_covers_body() {
        let body = Rect::new(0, 0, 80, 30);
        let plan = solve(
            body,
            &compose_plane(metrics()),
            &compose_modals(Some(SurfaceId::Agents), metrics(), body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::CoverBody
        ));
    }

    #[test]
    fn decide_is_centered() {
        let body = Rect::new(0, 0, 80, 30);
        let plan = solve(
            body,
            &compose_plane(metrics()),
            &compose_modals(Some(SurfaceId::Approval), metrics(), body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::Centered { .. }
        ));
        // Plane still present under decide dials.
        assert!(plan.rects.contains_key(&Region::Stream));
    }

    #[test]
    fn select_uses_composer_band() {
        let body = Rect::new(0, 0, 80, 30);
        let plan = solve(
            body,
            &compose_plane(metrics()),
            &compose_modals(Some(SurfaceId::Models), metrics(), body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::ComposerBand
        ));
    }
}
