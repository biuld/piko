//! Island ids and the default Workbench split tree.

/// First-class Workbench islands (layout atoms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IslandId {
    Sessions,
    Timeline,
    Composer,
    Agents,
    Tree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandAxis {
    Horizontal,
    Vertical,
}

/// Layout tree node: a single island or a split of children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IslandNode {
    Island(IslandId),
    Split {
        axis: IslandAxis,
        children: Vec<IslandNode>,
    },
}

impl IslandNode {
    pub fn island(id: IslandId) -> Self {
        Self::Island(id)
    }

    pub fn split(axis: IslandAxis, children: impl IntoIterator<Item = IslandNode>) -> Self {
        Self::Split {
            axis,
            children: children.into_iter().collect(),
        }
    }
}

/// Default docked Workbench tree (before visibility pruning).
///
/// Fixed product layout: Sessions | (Timeline/Composer) | (Agents/Tree).
/// The trailing vertical split is not a layout unit — only Agents and Tree are.
pub fn workbench_island_tree() -> IslandNode {
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

/// Drop closed islands and empty splits. Adjacent gutters disappear with slots.
pub fn prune_island_tree(
    node: &IslandNode,
    is_visible: &dyn Fn(IslandId) -> bool,
) -> Option<IslandNode> {
    match node {
        IslandNode::Island(id) => {
            if is_visible(*id) {
                Some(IslandNode::Island(*id))
            } else {
                None
            }
        }
        IslandNode::Split { axis, children } => {
            let kept: Vec<IslandNode> = children
                .iter()
                .filter_map(|child| prune_island_tree(child, is_visible))
                .collect();
            match kept.len() {
                0 => None,
                1 => kept.into_iter().next(),
                _ => Some(IslandNode::Split {
                    axis: *axis,
                    children: kept,
                }),
            }
        }
    }
}
