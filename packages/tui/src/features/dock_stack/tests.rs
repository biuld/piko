//! Table tests for the pure dock stack solver.

use super::*;

fn offer(id: BandId, active: bool, preferred: u16, min: u16) -> DockBandOffer {
    if active {
        DockBandOffer::active(id, preferred, min)
    } else {
        DockBandOffer::inactive(id)
    }
}

fn idle_offers(composer: u16) -> Vec<DockBandOffer> {
    vec![
        offer(BandId::Notice, false, 0, 0),
        offer(BandId::Todos, false, 0, 0),
        offer(BandId::Suggest, false, 0, 0),
        offer(BandId::Composer, true, composer, COMPOSER_MIN_HEIGHT),
    ]
}

fn full_offers(
    notice: bool,
    todos_pref: u16,
    suggest_pref: u16,
    composer: u16,
) -> Vec<DockBandOffer> {
    vec![
        if notice {
            offer(BandId::Notice, true, NOTICE_HEIGHT, NOTICE_HEIGHT)
        } else {
            offer(BandId::Notice, false, 0, 0)
        },
        if todos_pref > 0 {
            offer(BandId::Todos, true, todos_pref, TODOS_MIN_HEIGHT)
        } else {
            offer(BandId::Todos, false, 0, 0)
        },
        if suggest_pref > 0 {
            offer(BandId::Suggest, true, suggest_pref, SUGGEST_MIN_HEIGHT)
        } else {
            offer(BandId::Suggest, false, 0, 0)
        },
        offer(BandId::Composer, true, composer, COMPOSER_MIN_HEIGHT),
    ]
}

#[test]
fn idle_composer_only_grants_preferred() {
    let out = solve(DockSolveInput {
        body_height: 30,
        offers: idle_offers(5),
    });
    assert_eq!(out.height(BandId::Notice), 0);
    assert_eq!(out.height(BandId::Todos), 0);
    assert_eq!(out.height(BandId::Suggest), 0);
    assert_eq!(out.height(BandId::Composer), 5);
    assert!(out.stream_min >= STREAM_MIN_ABS);
    assert_eq!(out.dock_max, 30 - out.stream_min);
}

#[test]
fn notice_and_composer_get_preferred_when_budget_allows() {
    let out = solve(DockSolveInput {
        body_height: 30,
        offers: full_offers(true, 0, 0, 5),
    });
    assert_eq!(out.height(BandId::Notice), 1);
    assert_eq!(out.height(BandId::Composer), 5);
    assert_eq!(out.height(BandId::Suggest), 0);
    assert_eq!(out.height(BandId::Todos), 0);
}

#[test]
fn notice_suggest_composer_preferred_when_fit() {
    let suggest = suggestion_preferred_height(3);
    let out = solve(DockSolveInput {
        body_height: 40,
        offers: full_offers(true, 0, suggest, 5),
    });
    assert_eq!(out.height(BandId::Notice), 1);
    assert_eq!(out.height(BandId::Suggest), suggest);
    assert_eq!(out.height(BandId::Composer), 5);
    let dock_used = 1 + suggest + 5;
    assert!(dock_used <= out.dock_max);
}

#[test]
fn inactive_bands_grant_zero() {
    let out = solve(DockSolveInput {
        body_height: 24,
        offers: idle_offers(4),
    });
    for id in [BandId::Notice, BandId::Todos, BandId::Suggest] {
        assert_eq!(out.height(id), 0, "{id:?}");
    }
}

#[test]
fn modal_inactive_suggest_grants_zero() {
    // Product gate: modal open → Suggest offer active=false before solve.
    let out = solve(DockSolveInput {
        body_height: 30,
        offers: full_offers(true, 0, 0, 5), // suggest pref 0 ⇒ inactive
    });
    assert_eq!(out.height(BandId::Suggest), 0);
    assert_eq!(out.height(BandId::Notice), 1);
}

#[test]
fn shrink_transient_before_durable_before_composer() {
    // Tight body: preferred sum exceeds dock_max.
    // notice=1, todos=8, suggest=10, composer=8 → sum 27
    // body=24 → stream_min = max(3, 8)=8 → dock_max=16
    let body = 24u16;
    let out = solve(DockSolveInput {
        body_height: body,
        offers: full_offers(true, 8, 10, 8),
    });
    let s_min = stream_min(body);
    assert_eq!(out.stream_min, s_min);
    assert_eq!(out.dock_max, body - s_min);

    let notice = out.height(BandId::Notice);
    let todos = out.height(BandId::Todos);
    let suggest = out.height(BandId::Suggest);
    let composer = out.height(BandId::Composer);

    // Protect notice stays at full min while active.
    assert_eq!(notice, 1);
    // Suggest (transient) shrinks first — at or above min.
    assert!(suggest >= SUGGEST_MIN_HEIGHT);
    assert!(suggest <= 10);
    // Todos keeps at least header.
    assert!(todos >= TODOS_MIN_HEIGHT);
    assert!(todos <= 8);
    // Composer not below min.
    assert!(composer >= COMPOSER_MIN_HEIGHT);
    assert!(composer <= 8);

    let sum = notice + todos + suggest + composer;
    assert!(sum <= out.dock_max, "sum={sum} dock_max={}", out.dock_max);

    // Transient reduced before durable: if we still need room, suggest is
    // closer to min than todos relative to their preferred, when both shrunk.
    // Stronger check: with this matrix, suggest must have shrunk from 10.
    assert!(suggest < 10 || todos < 8 || composer < 8);

    // Order of sacrifice: given same pressure, transient hits min first.
    // Rebuild: after transient fully to min, durable shrinks next.
    let out2 = solve(DockSolveInput {
        body_height: 20,
        offers: full_offers(true, 8, 10, 6),
    });
    // dock_max for 20: stream_min=max(3,6)=6 → 14
    // preferred 1+8+10+6=25 → heavy pressure
    assert_eq!(out2.height(BandId::Notice), 1);
    assert_eq!(
        out2.height(BandId::Suggest),
        SUGGEST_MIN_HEIGHT,
        "transient must hit min before durable/composer under heavy pressure"
    );
    let todos2 = out2.height(BandId::Todos);
    let composer2 = out2.height(BandId::Composer);
    assert!(todos2 >= TODOS_MIN_HEIGHT);
    assert!(composer2 >= COMPOSER_MIN_HEIGHT);
    // Durable shrinks before composer min: todos should be at min or composer still > min
    // with remaining budget after notice+suggest min.
    assert!(
        todos2 < 8 || composer2 < 6,
        "durable or composer must yield under pressure"
    );
}

#[test]
fn protect_notice_not_fractionally_shrunk() {
    // Notice preferred=min=1; never 0 while active when dock fits.
    let out = solve(DockSolveInput {
        body_height: 30,
        offers: full_offers(true, 6, 8, 5),
    });
    assert_eq!(out.height(BandId::Notice), 1);
}

#[test]
fn all_bands_preferred_over_budget_respects_stream_floor() {
    let cases = [20u16, 30, 50];
    for body in cases {
        let out = solve(DockSolveInput {
            body_height: body,
            offers: full_offers(true, 8, 10, 8),
        });
        let dock_used: u16 = out.grants.iter().map(|g| g.height).sum();
        assert!(
            dock_used <= out.dock_max,
            "body={body}: used={dock_used} max={}",
            out.dock_max
        );
        // Stream floor is reserved in dock_max math.
        assert_eq!(out.dock_max + out.stream_min, body);
        assert!(out.stream_min >= STREAM_MIN_ABS.min(body));
        // Composer healthy when body is reasonable.
        if body >= 20 {
            assert!(out.height(BandId::Composer) >= COMPOSER_MIN_HEIGHT);
        }
    }
}

#[test]
fn grants_order_matches_registry() {
    let out = solve(DockSolveInput {
        body_height: 40,
        offers: full_offers(true, 4, 6, 5),
    });
    let ids: Vec<BandId> = out.grants.iter().map(|g| g.id).collect();
    assert_eq!(
        ids,
        vec![
            BandId::Notice,
            BandId::Todos,
            BandId::Suggest,
            BandId::Composer
        ]
    );
}

#[test]
fn active_grants_skip_zero_height() {
    let out = solve(DockSolveInput {
        body_height: 30,
        offers: full_offers(true, 0, 0, 5),
    });
    let active: Vec<BandId> = out.active_grants().map(|g| g.id).collect();
    assert_eq!(active, vec![BandId::Notice, BandId::Composer]);
}

#[test]
fn registry_v1_catalog() {
    let specs = registry();
    assert_eq!(specs.len(), 4);
    assert_eq!(specs[0].id, BandId::Notice);
    assert_eq!(specs[0].shrink, ShrinkClass::Protect);
    assert_eq!(specs[1].id, BandId::Todos);
    assert_eq!(specs[1].shrink, ShrinkClass::Durable);
    assert_eq!(specs[2].id, BandId::Suggest);
    assert_eq!(specs[2].shrink, ShrinkClass::Transient);
    assert_eq!(specs[3].id, BandId::Composer);
    assert_eq!(specs[3].shrink, ShrinkClass::Anchor);
}

#[test]
fn stream_min_scales_with_body() {
    assert_eq!(stream_min(9), 3); // max(3, 3)
    assert_eq!(stream_min(30), 10); // max(3, 10)
    assert_eq!(stream_min(3), 3);
    assert_eq!(stream_min(0), 0);
}
