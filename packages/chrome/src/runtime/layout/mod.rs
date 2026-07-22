//! Generic island split tree (layout atoms only).
//!
//! App code defines concrete leaf ids and default trees. Dock prefs, min widths,
//! and viewport fit policy stay in the app.

/// Split orientation for a layout node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandAxis {
    Horizontal,
    Vertical,
}

/// Layout tree node: a single island or a split of children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IslandNode<Id: Copy + Eq> {
    Island(Id),
    Split {
        axis: IslandAxis,
        children: Vec<IslandNode<Id>>,
    },
}

impl<Id: Copy + Eq> IslandNode<Id> {
    pub fn island(id: Id) -> Self {
        Self::Island(id)
    }

    pub fn split(axis: IslandAxis, children: impl IntoIterator<Item = IslandNode<Id>>) -> Self {
        Self::Split {
            axis,
            children: children.into_iter().collect(),
        }
    }
}

/// Drop closed islands and empty splits. Adjacent gutters disappear with slots.
pub fn prune_island_tree<Id: Copy + Eq>(
    node: &IslandNode<Id>,
    is_visible: &dyn Fn(Id) -> bool,
) -> Option<IslandNode<Id>> {
    match node {
        IslandNode::Island(id) => {
            if is_visible(*id) {
                Some(IslandNode::Island(*id))
            } else {
                None
            }
        }
        IslandNode::Split { axis, children } => {
            let kept: Vec<IslandNode<Id>> = children
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Id {
        A,
        B,
        C,
    }

    #[test]
    fn prune_drops_closed_and_collapses() {
        let tree = IslandNode::split(
            IslandAxis::Horizontal,
            [
                IslandNode::island(Id::A),
                IslandNode::split(
                    IslandAxis::Vertical,
                    [IslandNode::island(Id::B), IslandNode::island(Id::C)],
                ),
            ],
        );
        let pruned = prune_island_tree(&tree, &|id| matches!(id, Id::B | Id::C));
        assert_eq!(
            pruned,
            Some(IslandNode::split(
                IslandAxis::Vertical,
                [IslandNode::island(Id::B), IslandNode::island(Id::C)],
            ))
        );

        let only_b = prune_island_tree(&tree, &|id| id == Id::B);
        assert_eq!(only_b, Some(IslandNode::island(Id::B)));
    }
}
