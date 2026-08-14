//! Dock Stack — plane ephemeral-band coordinator.
//!
//! Registers dock bands, collects per-frame offers from provider features,
//! solves joint height under Stream floor / dock max, and emits grants for
//! `compose_plane`. Product features never allocate plane siblings outside
//! this path.
//!
//! Not to be confused with:
//! - `SurfaceIntent::Dock` (modal ComposerBand for approval / tool workflow)
//! - `ui::components::dock_line` (single-row paint helper)

mod band;
mod grant;
mod offer;
mod registry;
mod solve;

#[cfg(test)]
mod tests;

pub use band::BandId;
#[allow(unused_imports)] // public API
pub use band::{BandSpec, Residency, ShrinkClass};
#[allow(unused_imports)] // public API
pub use grant::DockBandGrant;
pub use grant::DockSolveOutput;
pub use offer::DockBandOffer;
pub use registry::{
    COMPOSER_MIN_HEIGHT, DOCK_BOUNDARY_HEIGHT, GUIDANCE_HEIGHT, SUGGEST_MIN_HEIGHT,
    SUGGEST_SHARED_BOUNDARY_MIN_HEIGHT, TODOS_MAX_ITEM_ROWS, TODOS_MIN_HEIGHT,
    suggestion_preferred_height,
};
#[allow(unused_imports)] // public API + tests
pub use registry::{STREAM_MIN_ABS, registry, stream_min};
pub use solve::{DockSolveInput, solve};
