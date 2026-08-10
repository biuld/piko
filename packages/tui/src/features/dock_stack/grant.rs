//! Solver grants: height allocated to each band this frame.

use super::band::BandId;

/// What the stack allows for one band (0 ⇒ omit region leaf).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DockBandGrant {
    pub id: BandId,
    pub height: u16,
}

/// Full solver output for plane composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DockSolveOutput {
    /// Grants in registry top→bottom order.
    pub grants: Vec<DockBandGrant>,
    pub stream_min: u16,
    pub dock_max: u16,
}

impl DockSolveOutput {
    /// Height granted to `id`, or 0 if absent.
    pub fn height(&self, id: BandId) -> u16 {
        self.grants
            .iter()
            .find(|g| g.id == id)
            .map(|g| g.height)
            .unwrap_or(0)
    }

    /// Grants with positive height, in stack order (for compose).
    pub fn active_grants(&self) -> impl Iterator<Item = DockBandGrant> + '_ {
        self.grants.iter().copied().filter(|g| g.height > 0)
    }
}
