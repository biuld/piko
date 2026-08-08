//! Product navigation: surfaces, focus targets, body regions, layout trees.
//!
//! Layout engine primitives: `piko-tui-layout` (no product ids).

mod compose;
mod focus_target;
mod region;
mod surface;

pub use compose::{PlaneMetrics, compose_modals, compose_plane};
pub use focus_target::AppMode;
pub use region::Region;
pub use surface::{SurfaceId, SurfaceIntent};

pub type FocusManager = piko_tui_layout::FocusManager<AppMode>;

pub trait FocusManagerExt {
    fn active_surface(&self) -> Option<SurfaceId>;
}

impl FocusManagerExt for FocusManager {
    fn active_surface(&self) -> Option<SurfaceId> {
        self.active().as_surface()
    }
}
