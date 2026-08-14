//! Static v1 band catalog and order.

use super::band::{BandId, BandSpec, Residency, ShrinkClass};

/// Absolute floor for Stream height (rows).
pub const STREAM_MIN_ABS: u16 = 3;

/// Stream floor is at least this fraction of body height (`num / den`).
pub const STREAM_MIN_RATIO_NUM: u16 = 1;
pub const STREAM_MIN_RATIO_DEN: u16 = 3;

/// Composer content + chrome when empty (1 content row + top/bottom pad).
pub const COMPOSER_MIN_HEIGHT: u16 = 3;

/// Suggest chrome + one content row while the palette is open. Its guidance
/// projects through the resident Guidance row.
pub const SUGGEST_MIN_HEIGHT: u16 = 3;

/// Resident Guidance row directly above Composer.
pub const GUIDANCE_HEIGHT: u16 = 1;

/// Todos header-only min while the strip is active.
pub const TODOS_MIN_HEIGHT: u16 = 1;

/// Max item rows painted in the Todos strip (not counting header / overflow).
pub const TODOS_MAX_ITEM_ROWS: u16 = 6;

/// v1 registry: Todos → Suggest → Guidance → Composer (top → bottom).
pub fn registry() -> &'static [BandSpec] {
    &REGISTRY
}

const REGISTRY: [BandSpec; 4] = [
    BandSpec {
        id: BandId::Todos,
        order: 1,
        residency: Residency::Ephemeral,
        shrink: ShrinkClass::Durable,
    },
    BandSpec {
        id: BandId::Suggest,
        order: 2,
        residency: Residency::Ephemeral,
        shrink: ShrinkClass::Transient,
    },
    BandSpec {
        id: BandId::Guidance,
        order: 3,
        residency: Residency::Anchor,
        shrink: ShrinkClass::Protect,
    },
    BandSpec {
        id: BandId::Composer,
        order: 4,
        residency: Residency::Anchor,
        shrink: ShrinkClass::Anchor,
    },
];

/// Stream floor for a given plane body height.
pub fn stream_min(body_height: u16) -> u16 {
    let ratio = body_height.saturating_mul(STREAM_MIN_RATIO_NUM) / STREAM_MIN_RATIO_DEN.max(1);
    STREAM_MIN_ABS.max(ratio).min(body_height)
}

/// Preferred height for the Suggest band (existing palette formula).
pub fn suggestion_preferred_height(count: usize) -> u16 {
    let rows = (count.max(1) as u16).min(6);
    (rows + 2).min(9)
}
