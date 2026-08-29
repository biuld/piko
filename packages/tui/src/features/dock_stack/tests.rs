//! Table tests for the pure dock stack solver.

use super::*;

fn offers(suggest: u16, composer: u16) -> Vec<DockBandOffer> {
    vec![
        DockBandOffer::active(BandId::Boundary, 1, 1),
        if suggest == 0 {
            DockBandOffer::inactive(BandId::Suggest)
        } else {
            DockBandOffer::active(BandId::Suggest, suggest, SUGGEST_MIN_HEIGHT)
        },
        DockBandOffer::active(BandId::Guidance, 1, 1),
        DockBandOffer::active(BandId::Composer, composer, COMPOSER_MIN_HEIGHT),
    ]
}

#[test]
fn idle_uses_reserved_boundary_guidance_and_composer() {
    let out = solve(DockSolveInput {
        body_height: 30,
        offers: offers(0, 5),
    });
    assert_eq!(out.height(BandId::Boundary), 1);
    assert_eq!(out.height(BandId::Suggest), 0);
    assert_eq!(out.height(BandId::Guidance), 1);
    assert_eq!(out.height(BandId::Composer), 5);
    assert_eq!(out.dock_max + out.stream_min, 30);
}

#[test]
fn suggest_shrinks_before_composer() {
    let out = solve(DockSolveInput {
        body_height: 20,
        offers: offers(10, 8),
    });
    assert_eq!(out.height(BandId::Boundary), 1);
    assert_eq!(out.height(BandId::Guidance), 1);
    assert!(out.height(BandId::Suggest) >= SUGGEST_MIN_HEIGHT);
    assert!(out.height(BandId::Composer) >= COMPOSER_MIN_HEIGHT);
    let used: u16 = out.grants.iter().map(|grant| grant.height).sum();
    assert!(used <= out.dock_max);
}

#[test]
fn grants_follow_registry_order() {
    let out = solve(DockSolveInput {
        body_height: 40,
        offers: offers(4, 5),
    });
    let ids: Vec<_> = out.grants.iter().map(|grant| grant.id).collect();
    assert_eq!(
        ids,
        vec![
            BandId::Boundary,
            BandId::Suggest,
            BandId::Guidance,
            BandId::Composer
        ]
    );
}

#[test]
fn registry_has_no_todo_band() {
    let specs = registry();
    assert_eq!(specs.len(), 4);
    assert_eq!(specs[0].id, BandId::Boundary);
    assert_eq!(specs[1].id, BandId::Suggest);
    assert_eq!(specs[2].id, BandId::Guidance);
    assert_eq!(specs[3].id, BandId::Composer);
}

#[test]
fn stream_min_scales_with_body() {
    assert_eq!(stream_min(9), 3);
    assert_eq!(stream_min(30), 10);
    assert_eq!(stream_min(3), 3);
    assert_eq!(stream_min(0), 0);
}
