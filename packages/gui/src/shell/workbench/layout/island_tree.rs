//! Piko Workbench island ids and the default split tree.
//!
//! Generic tree/prune live in [`piko_chrome::layout`]. Product leaf ids and the
//! fixed five-island preset stay here.

/// First-class Workbench islands (layout atoms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IslandId {
    Sessions,
    Timeline,
    Composer,
    Agents,
    Tree,
}

/// Every focusable Workbench island (must match [`crate::shell::focus_order`]
/// membership and `IslandFocusTable` registration).
pub const ALL_ISLAND_IDS: [IslandId; 5] = [
    IslandId::Sessions,
    IslandId::Timeline,
    IslandId::Composer,
    IslandId::Agents,
    IslandId::Tree,
];

pub use piko_chrome::layout::{IslandAxis, IslandNode, prune_island_tree};

/// Default docked Workbench tree (before visibility pruning).
///
/// Fixed product layout: Sessions | (Timeline/Composer) | (Agents/Tree).
/// The trailing vertical split is not a layout unit — only Agents and Tree are.
pub fn workbench_island_tree() -> IslandNode<IslandId> {
    IslandNode::split(
        IslandAxis::Horizontal,
        [
            IslandNode::island(IslandId::Sessions),
            IslandNode::split(
                IslandAxis::Vertical,
                [
                    IslandNode::island(IslandId::Timeline),
                    IslandNode::island(IslandId::Composer),
                ],
            ),
            IslandNode::split(
                IslandAxis::Vertical,
                [
                    IslandNode::island(IslandId::Agents),
                    IslandNode::island(IslandId::Tree),
                ],
            ),
        ],
    )
}
