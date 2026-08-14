//! Band identity, residency, shrink class, and registry specs.

use crate::navigation::Region;

/// Stable band identity in the plane dock stack.
///
/// Stream is not a band offer — it is the grow anchor handled beside solve.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum BandId {
    Boundary,
    Todos,
    Suggest,
    Guidance,
    Composer,
}

/// Whether the band always participates in layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Residency {
    /// Height 0 when inactive.
    Ephemeral,
    /// Always participates (Composer).
    Anchor,
}

/// How the solver sacrifices height under pressure (feature PRD order).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ShrinkClass {
    /// Suggest / command palette / @ browser — shrink first.
    Transient,
    /// Todos strip — keep min (header) while active.
    Durable,
    /// Composer — shrink toward editor min after ephemeral classes.
    Anchor,
    /// Single-row resident boundary / guidance — preserved unless pathological.
    Protect,
}

/// Static description of one registered dock band.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BandSpec {
    pub id: BandId,
    /// Top-to-bottom order among dock bands (not including Stream).
    pub order: u8,
    pub residency: Residency,
    pub shrink: ShrinkClass,
}

impl BandId {
    /// Map band id to the paint [`Region`] leaf.
    pub fn region(self) -> Region {
        match self {
            BandId::Boundary => Region::DockBoundary,
            BandId::Todos => Region::Todos,
            BandId::Suggest => Region::Suggest,
            BandId::Guidance => Region::Guidance,
            BandId::Composer => Region::Composer,
        }
    }
}
