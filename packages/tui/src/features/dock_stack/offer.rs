//! Per-frame height offers from band providers.

use super::band::BandId;

/// What a provider wants this frame (pure projection of its domain state).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DockBandOffer {
    pub id: BandId,
    /// False → treat preferred/min as 0 (band omitted).
    pub active: bool,
    pub preferred_height: u16,
    /// Honored while active if dock_max allows (after higher-yield shrinks).
    pub min_height: u16,
}

impl DockBandOffer {
    /// Inactive band: height 0, still registered so the solver sees a slot.
    pub fn inactive(id: BandId) -> Self {
        Self {
            id,
            active: false,
            preferred_height: 0,
            min_height: 0,
        }
    }

    /// Active band with preferred and min heights (preferred clamped ≥ min).
    pub fn active(id: BandId, preferred_height: u16, min_height: u16) -> Self {
        let min_height = min_height.max(1);
        Self {
            id,
            active: true,
            preferred_height: preferred_height.max(min_height),
            min_height,
        }
    }

    /// Normalize for the solver: inactive → zeros; active clamps preferred ≥ min.
    pub(crate) fn normalized(self) -> Self {
        if !self.active {
            return Self {
                id: self.id,
                active: false,
                preferred_height: 0,
                min_height: 0,
            };
        }
        let min_height = self.min_height.max(1);
        Self {
            id: self.id,
            active: true,
            preferred_height: self.preferred_height.max(min_height),
            min_height,
        }
    }
}
