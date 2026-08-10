//! Pure dock stack height solver.

use super::band::{BandId, ShrinkClass};
use super::grant::{DockBandGrant, DockSolveOutput};
use super::offer::DockBandOffer;
use super::registry::{registry, stream_min};

/// Input to [`solve`].
#[derive(Clone, Debug)]
pub struct DockSolveInput {
    pub body_height: u16,
    pub offers: Vec<DockBandOffer>,
}

/// Arbitrate offers under Stream floor / dock max.
///
/// Shrink order: Transient → Durable → Anchor (Composer). Protect (Notice)
/// stays at min while active. Inactive bands grant 0.
pub fn solve(input: DockSolveInput) -> DockSolveOutput {
    let specs = registry();
    let stream_min = stream_min(input.body_height);
    let dock_max = input.body_height.saturating_sub(stream_min);

    // Align offers to registry order; missing → inactive.
    let mut working: Vec<(BandId, DockBandOffer, ShrinkClass, u16)> = specs
        .iter()
        .map(|spec| {
            let offer = input
                .offers
                .iter()
                .find(|o| o.id == spec.id)
                .copied()
                .unwrap_or_else(|| DockBandOffer::inactive(spec.id))
                .normalized();
            let height = if offer.active {
                offer.preferred_height
            } else {
                0
            };
            (spec.id, offer, spec.shrink, height)
        })
        .collect();

    let mut sum: u16 = working.iter().map(|(_, _, _, h)| *h).sum();

    // Shrink by class order while over budget.
    for class in [
        ShrinkClass::Transient,
        ShrinkClass::Durable,
        ShrinkClass::Anchor,
    ] {
        shrink_class(&mut working, &mut sum, dock_max, class);
    }
    // Protect never shrinks below min while active — already at preferred==min.

    // Final clamp: if still over budget (pathological tiny body), compress
    // non-protect non-zero bands toward min, then protect last only if needed
    // to fit (rare: body_height < notice+composer mins).
    if sum > dock_max {
        emergency_fit(&mut working, &mut sum, dock_max);
    }

    let grants = working
        .into_iter()
        .map(|(id, _, _, height)| DockBandGrant { id, height })
        .collect();

    DockSolveOutput {
        grants,
        stream_min,
        dock_max,
    }
}

fn shrink_class(
    working: &mut [(BandId, DockBandOffer, ShrinkClass, u16)],
    sum: &mut u16,
    dock_max: u16,
    class: ShrinkClass,
) {
    while *sum > dock_max {
        let mut progressed = false;
        for entry in working.iter_mut() {
            if entry.2 != class {
                continue;
            }
            let offer = entry.1;
            if !offer.active || entry.3 <= offer.min_height {
                continue;
            }
            entry.3 -= 1;
            *sum = sum.saturating_sub(1);
            progressed = true;
            if *sum <= dock_max {
                break;
            }
        }
        if !progressed {
            break;
        }
    }
}

fn emergency_fit(
    working: &mut [(BandId, DockBandOffer, ShrinkClass, u16)],
    sum: &mut u16,
    dock_max: u16,
) {
    // First drive every non-protect band to min.
    for class in [
        ShrinkClass::Transient,
        ShrinkClass::Durable,
        ShrinkClass::Anchor,
    ] {
        shrink_class(working, sum, dock_max, class);
    }
    // Only if still over: drop protect to 0 (body too small for notice+composer).
    if *sum > dock_max {
        for entry in working.iter_mut() {
            if entry.2 == ShrinkClass::Protect && entry.3 > 0 {
                *sum = sum.saturating_sub(entry.3);
                entry.3 = 0;
            }
        }
    }
    // Absolute last resort: force heights down to fit (keeps composer if possible).
    while *sum > dock_max {
        let mut progressed = false;
        // Prefer reducing non-composer first.
        for prefer_composer_last in [false, true] {
            for entry in working.iter_mut() {
                if entry.3 == 0 {
                    continue;
                }
                if !prefer_composer_last && entry.0 == BandId::Composer {
                    continue;
                }
                if prefer_composer_last && entry.0 != BandId::Composer {
                    continue;
                }
                entry.3 -= 1;
                *sum = sum.saturating_sub(1);
                progressed = true;
                if *sum <= dock_max {
                    break;
                }
            }
            if *sum <= dock_max {
                break;
            }
        }
        if !progressed {
            break;
        }
    }
}
