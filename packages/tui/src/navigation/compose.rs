//! Product layout trees with `piko-tui-layout` primitives only.
//!
//! Workspace plane = Stream + optional Notice/Suggest + Composer.
//! Surfaces = modal z-stack (Browse / Select / Decide).

use piko_tui_layout::{FlexItem, ModalLayer, Node, flex_column, leaf};
use ratatui::layout::Rect;

use super::select_band::SelectBandBudget;
use super::{Region, SurfaceId, SurfaceIntent};

/// Per-frame sizes from feature state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaneMetrics {
    pub notice: bool,
    pub suggest: bool,
    pub suggestion_count: usize,
    pub composer_height: u16,
    pub body_height: u16,
    /// When the active modal is Select: content-row budget for ComposerBand.
    pub select_band: Option<SelectBandBudget>,
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
    let budget = m
        .select_band
        .unwrap_or_else(|| SelectBandBudget::minimal_stacked_list(0));
    budget.resolve_band_rows(m.body_height)
}

fn suggestion_height(count: usize) -> u16 {
    // Minimal pane: top/bottom borders + content rows (empty → 1) + footer hints.
    let rows = (count.max(1) as u16).min(6);
    (rows + 3).min(10)
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
            select_band: None,
        }
    }

    fn metrics_with_budget(budget: SelectBandBudget) -> PlaneMetrics {
        PlaneMetrics {
            select_band: Some(budget),
            ..metrics()
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
            &compose_modals(Some(SurfaceId::Sessions), metrics(), body),
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

    #[test]
    fn select_band_height_follows_content_rows() {
        let body = Rect::new(0, 0, 80, 30);
        let budget = SelectBandBudget::minimal_stacked_list(3);
        let expected = budget.resolve_band_rows(30);
        let plan = solve(
            body,
            &compose_plane(metrics_with_budget(budget)),
            &compose_modals(Some(SurfaceId::Models), metrics_with_budget(budget), body),
        );
        let layer = &plan.layers[0];
        assert!(matches!(layer.placement, ModalPlacement::ComposerBand));
        let surface_h = layer.rects.values().map(|r| r.height).max().unwrap_or(0);
        assert_eq!(surface_h, expected);
        // Prefer content budget over legacy ~40% body (would be 12+).
        assert_eq!(expected, 10); // 4 chrome + 3×2 content
    }

    #[test]
    fn agents_uses_composer_band() {
        let body = Rect::new(0, 0, 80, 30);
        let plan = solve(
            body,
            &compose_plane(metrics()),
            &compose_modals(Some(SurfaceId::Agents), metrics(), body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::ComposerBand
        ));
        assert!(plan.rects.contains_key(&Region::Stream));
    }

    #[test]
    fn thinking_uses_composer_band() {
        let body = Rect::new(0, 0, 80, 30);
        let plan = solve(
            body,
            &compose_plane(metrics()),
            &compose_modals(Some(SurfaceId::Thinking), metrics(), body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::ComposerBand
        ));
    }
}
