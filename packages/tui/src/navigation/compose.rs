//! Product layout trees with `piko-tui-layout` primitives only.
//!
//! Workspace plane = Stream + dock stack grants
//! (Boundary/Todos/Suggest/Guidance/Composer).
//! Surfaces = modal z-stack (Browse / Select / Dock / Modal).
//!
//! Dock band heights come only from [`crate::features::dock_stack`] grants —
//! never ad-hoc fixed stacking that ignores joint budget.

use piko_tui_layout::{FlexItem, ModalLayer, Node, flex_column, leaf};
use ratatui::layout::Rect;

use crate::features::dock_stack::DockSolveOutput;

use super::select_band::SelectBandBudget;
use super::{Region, SurfaceId, SurfaceIntent};

/// Per-frame sizes from feature state + dock stack solution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaneMetrics {
    /// Dock stack grants (Boundary / Todos / Suggest / Guidance / Composer).
    pub dock: DockSolveOutput,
    pub body_height: u16,
    /// When the active modal is Select / Dock: content-row budget for
    /// ComposerBand.
    pub select_band: Option<SelectBandBudget>,
    /// Preferred centered size (w, h) of the active Modal surface, when known.
    pub centered_size: Option<(u16, u16)>,
}

impl PlaneMetrics {
    #[allow(dead_code)]
    pub fn guidance(&self) -> bool {
        self.dock
            .height(crate::features::dock_stack::BandId::Guidance)
            > 0
    }

    #[allow(dead_code)]
    pub fn suggest(&self) -> bool {
        self.dock
            .height(crate::features::dock_stack::BandId::Suggest)
            > 0
    }

    #[allow(dead_code)]
    pub fn todos(&self) -> bool {
        self.dock.height(crate::features::dock_stack::BandId::Todos) > 0
    }

    #[allow(dead_code)]
    pub fn composer_height(&self) -> u16 {
        self.dock
            .height(crate::features::dock_stack::BandId::Composer)
    }
}

/// Stable workspace plane (always composed). Heights come only from grants.
pub fn compose_plane(m: &PlaneMetrics) -> Node<Region> {
    let mut children = vec![FlexItem::grow(1, leaf(Region::Stream))];
    for grant in m.dock.active_grants() {
        children.push(FlexItem::fixed(grant.height, leaf(grant.id.region())));
    }
    flex_column(children)
}

/// Host-priority + focus → modal layers (at most one product surface for now).
pub fn compose_modals(
    host_surface: Option<SurfaceId>,
    metrics: &PlaneMetrics,
    body: Rect,
) -> Vec<ModalLayer<Region>> {
    let Some(surface) = host_surface else {
        return Vec::new();
    };
    let band = select_band_height(surface, metrics);
    vec![surface.modal_layer(body, band, metrics.centered_size)]
}

fn select_band_height(surface: SurfaceId, m: &PlaneMetrics) -> u16 {
    if !matches!(
        surface.intent(),
        SurfaceIntent::Select | SurfaceIntent::Dock
    ) {
        return 0;
    }
    let budget = m
        .select_band
        .unwrap_or_else(|| SelectBandBudget::minimal_stacked_list(0));
    budget.resolve_band_rows(m.body_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::dock_stack::{
        BandId, COMPOSER_MIN_HEIGHT, DOCK_BOUNDARY_HEIGHT, DockBandOffer, DockSolveInput,
        GUIDANCE_HEIGHT, SUGGEST_MIN_HEIGHT, solve, suggestion_preferred_height,
    };
    use piko_tui_layout::{ModalPlacement, solve as layout_solve, solve_flex};
    use ratatui::layout::Rect;

    fn metrics_from_offers(body: u16, offers: Vec<DockBandOffer>) -> PlaneMetrics {
        let dock = solve(DockSolveInput {
            body_height: body,
            offers,
        });
        PlaneMetrics {
            dock,
            body_height: body,
            select_band: None,
            centered_size: None,
        }
    }

    fn idle_metrics() -> PlaneMetrics {
        metrics_from_offers(
            30,
            vec![
                DockBandOffer::active(BandId::Boundary, DOCK_BOUNDARY_HEIGHT, DOCK_BOUNDARY_HEIGHT),
                DockBandOffer::inactive(BandId::Todos),
                DockBandOffer::inactive(BandId::Suggest),
                DockBandOffer::active(BandId::Guidance, GUIDANCE_HEIGHT, GUIDANCE_HEIGHT),
                DockBandOffer::active(BandId::Composer, 5, COMPOSER_MIN_HEIGHT),
            ],
        )
    }

    fn metrics_with_budget(budget: SelectBandBudget) -> PlaneMetrics {
        let mut m = idle_metrics();
        m.select_band = Some(budget);
        m
    }

    #[test]
    fn plane_is_stream_guidance_and_composer() {
        let m = idle_metrics();
        let rects = solve_flex(Rect::new(0, 0, 80, 30), &compose_plane(&m));
        assert!(rects.contains_key(&Region::Stream));
        assert!(rects.contains_key(&Region::Composer));
        assert_eq!(rects.get(&Region::Guidance).map(|r| r.height), Some(1));
        assert!(!rects.keys().any(|r| matches!(r, Region::Surface(_))));
        assert_eq!(
            rects.get(&Region::DockBoundary).map(|r| r.height),
            Some(DOCK_BOUNDARY_HEIGHT)
        );
        assert_eq!(rects.get(&Region::Stream).map(|r| r.height), Some(23));
        assert_eq!(rects.get(&Region::Composer).map(|r| r.height), Some(5));
    }

    #[test]
    fn plane_grants_drive_fixed_heights_and_order() {
        let suggest = suggestion_preferred_height(2, false);
        let m = metrics_from_offers(
            40,
            vec![
                DockBandOffer::active(BandId::Boundary, DOCK_BOUNDARY_HEIGHT, DOCK_BOUNDARY_HEIGHT),
                DockBandOffer::active(BandId::Todos, 4, 1),
                DockBandOffer::active(BandId::Suggest, suggest, SUGGEST_MIN_HEIGHT),
                DockBandOffer::active(BandId::Guidance, GUIDANCE_HEIGHT, GUIDANCE_HEIGHT),
                DockBandOffer::active(BandId::Composer, 5, COMPOSER_MIN_HEIGHT),
            ],
        );
        let node = compose_plane(&m);
        let rects = solve_flex(Rect::new(0, 0, 80, 40), &node);
        assert!(rects.contains_key(&Region::Stream));
        assert_eq!(rects[&Region::Todos].height, 4);
        assert_eq!(rects[&Region::Suggest].height, suggest);
        assert_eq!(rects[&Region::Guidance].height, 1);
        assert_eq!(rects[&Region::Composer].height, 5);
        // Order: Stream < Boundary < Todos < Suggest < Guidance < Composer.
        let ys = [
            rects[&Region::Stream].y,
            rects[&Region::DockBoundary].y,
            rects[&Region::Todos].y,
            rects[&Region::Suggest].y,
            rects[&Region::Guidance].y,
            rects[&Region::Composer].y,
        ];
        assert!(ys.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn plane_omits_zero_grant_regions() {
        let m = idle_metrics();
        let rects = solve_flex(Rect::new(0, 0, 80, 30), &compose_plane(&m));
        assert!(!rects.contains_key(&Region::Todos));
        assert!(!rects.contains_key(&Region::Suggest));
        assert!(rects.contains_key(&Region::DockBoundary));
        assert!(rects.contains_key(&Region::Guidance));
    }

    #[test]
    fn short_body_keeps_stream_floor() {
        let m = metrics_from_offers(
            20,
            vec![
                DockBandOffer::active(BandId::Boundary, DOCK_BOUNDARY_HEIGHT, DOCK_BOUNDARY_HEIGHT),
                DockBandOffer::active(BandId::Todos, 8, 1),
                DockBandOffer::active(BandId::Suggest, 10, SUGGEST_MIN_HEIGHT),
                DockBandOffer::active(BandId::Guidance, 1, 1),
                DockBandOffer::active(BandId::Composer, 8, COMPOSER_MIN_HEIGHT),
            ],
        );
        let rects = solve_flex(Rect::new(0, 0, 80, 20), &compose_plane(&m));
        let stream_h = rects[&Region::Stream].height;
        assert!(
            stream_h >= m.dock.stream_min,
            "stream={stream_h} min={}",
            m.dock.stream_min
        );
        let dock_h: u16 = [
            Region::DockBoundary,
            Region::Todos,
            Region::Suggest,
            Region::Guidance,
            Region::Composer,
        ]
        .iter()
        .filter_map(|r| rects.get(r).map(|rect| rect.height))
        .sum();
        assert!(dock_h <= m.dock.dock_max);
    }

    #[test]
    fn browse_covers_body() {
        let body = Rect::new(0, 0, 80, 30);
        let metrics = idle_metrics();
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::Sessions), &metrics, body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::CoverBody
        ));
    }

    #[test]
    fn approval_uses_composer_band() {
        let body = Rect::new(0, 0, 80, 30);
        let metrics = metrics_with_budget(SelectBandBudget::standard_info(7));
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::Approval), &metrics, body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::ComposerBand
        ));
        assert!(plan.rects.contains_key(&Region::Stream));
    }

    #[test]
    fn tool_interaction_uses_composer_band() {
        let body = Rect::new(0, 0, 80, 30);
        let metrics = metrics_with_budget(SelectBandBudget::standard_info(6));
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::ToolInteraction), &metrics, body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::ComposerBand
        ));
        assert!(plan.rects.contains_key(&Region::Stream));
    }

    #[test]
    fn settings_is_centered_modal() {
        let body = Rect::new(0, 0, 80, 30);
        let mut metrics = idle_metrics();
        metrics.centered_size = Some((60, 24));
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::Settings), &metrics, body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::Centered { .. }
        ));
        assert!(plan.rects.contains_key(&Region::Stream));
    }

    #[test]
    fn usage_is_centered_modal() {
        let body = Rect::new(0, 0, 80, 30);
        let mut metrics = idle_metrics();
        metrics.centered_size = Some((60, 11));
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::Usage), &metrics, body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::Centered {
                max_width: 60,
                max_height: 11
            }
        ));
        assert!(plan.rects.contains_key(&Region::Stream));
    }

    #[test]
    fn notifications_is_centered_modal() {
        let body = Rect::new(0, 0, 80, 30);
        let mut metrics = idle_metrics();
        metrics.centered_size = Some((70, 18));
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::Notifications), &metrics, body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::Centered { .. }
        ));
        assert!(plan.rects.contains_key(&Region::Stream));
    }

    #[test]
    fn select_uses_composer_band() {
        let body = Rect::new(0, 0, 80, 30);
        let metrics = idle_metrics();
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::Models), &metrics, body),
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
        let metrics = metrics_with_budget(budget);
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::Models), &metrics, body),
        );
        let layer = &plan.layers[0];
        assert!(matches!(layer.placement, ModalPlacement::ComposerBand));
        let surface_h = layer.rects.values().map(|r| r.height).max().unwrap_or(0);
        assert_eq!(surface_h, expected);
        assert_eq!(expected, 9); // 3 chrome + 3×2 content
    }

    #[test]
    fn agents_uses_composer_band() {
        let body = Rect::new(0, 0, 80, 30);
        let metrics = idle_metrics();
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::Agents), &metrics, body),
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
        let metrics = idle_metrics();
        let plan = layout_solve(
            body,
            &compose_plane(&metrics),
            &compose_modals(Some(SurfaceId::Thinking), &metrics, body),
        );
        assert!(matches!(
            plan.layers[0].placement,
            ModalPlacement::ComposerBand
        ));
    }

    #[test]
    fn command_result_lists_use_composer_band() {
        let body = Rect::new(0, 0, 80, 30);
        for surface in [SurfaceId::Mcp, SurfaceId::Processes] {
            let metrics = metrics_with_budget(SelectBandBudget::standard_info(4));
            let plan = layout_solve(
                body,
                &compose_plane(&metrics),
                &compose_modals(Some(surface), &metrics, body),
            );
            assert!(matches!(
                plan.layers[0].placement,
                ModalPlacement::ComposerBand
            ));
            assert!(plan.rects.contains_key(&Region::Stream));
        }
    }
}
