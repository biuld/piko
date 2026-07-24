//! Island shell for the piko Workbench.
//!
//! Panel / focus primitives come from [`piko_chrome`]. Product messages and
//! session-phase mapping stay here so `piko-chrome` never depends on Client Core.

mod msg;
mod phase;

use std::ops::{Deref, DerefMut};

pub use msg::IslandMsg;
pub use phase::IslandSessionPhase;
pub use piko_chrome::components::panel::{
    IslandBody, IslandContentViewport, IslandHeader, IslandMedia, IslandPanel, IslandPlaceholder,
};
pub use piko_chrome::runtime::island::{
    FocusCycleDir, FocusReason, FocusRing, IslandFocusTable, IslandHost, IslandMessage, IslandView,
    activate_focus_handle, route_focus_message, schedule_island_message,
};

use crate::shell::workbench::IslandId;

/// Workbench focus ring specialized to piko [`IslandId`].
///
/// Newtype so we can implement [`Default`] without orphan-rule issues on
/// [`FocusRing`].
#[derive(Debug, Clone)]
pub struct IslandFocusRing(FocusRing<IslandId>);

impl Default for IslandFocusRing {
    fn default() -> Self {
        Self(FocusRing::new(IslandId::Sessions))
    }
}

impl Deref for IslandFocusRing {
    type Target = FocusRing<IslandId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for IslandFocusRing {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
