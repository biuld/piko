//! Pure layout tree tests.

use super::island_tree::{IslandAxis, IslandNode};
use super::*;

#[test]
fn breakpoints() {
    assert_eq!(LayoutBreakpoint::from_width(1300.), LayoutBreakpoint::Wide);
    assert_eq!(
        LayoutBreakpoint::from_width(1000.),
        LayoutBreakpoint::Medium
    );
    assert_eq!(LayoutBreakpoint::from_width(800.), LayoutBreakpoint::Narrow);
}

#[test]
fn prune_drops_closed_islands_and_empty_splits() {
    let tree = workbench_island_tree();
    let pruned = prune_island_tree(&tree, &|id| {
        matches!(
            id,
            IslandId::Timeline | IslandId::Composer | IslandId::Sessions
        )
    });
    assert_eq!(
        pruned,
        Some(IslandNode::split(
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
            ],
        ))
    );
}

#[test]
fn prune_collapses_single_child_split() {
    let tree = IslandNode::split(
        IslandAxis::Horizontal,
        [
            IslandNode::island(IslandId::Sessions),
            IslandNode::island(IslandId::Timeline),
        ],
    );
    let pruned = prune_island_tree(&tree, &|id| id == IslandId::Timeline);
    assert_eq!(pruned, Some(IslandNode::island(IslandId::Timeline)));
}

#[test]
fn medium_hides_right_keeps_sessions_pref() {
    let mut s = IslandLayoutState::default();
    s.sync_breakpoint(1000.);
    assert!(s.is_docked_visible(IslandId::Sessions, true));
    assert!(!s.is_docked_visible(IslandId::Agents, true));
    assert!(!s.any_right_column_docked(true));
    s.sessions_open = false;
    assert!(!s.is_docked_visible(IslandId::Sessions, true));
}

#[test]
fn wide_respects_right_column_pref() {
    let mut s = IslandLayoutState::default();
    s.sync_breakpoint(1400.);
    s.toggle_right_column();
    assert!(!s.any_right_column_docked(true));
    s.toggle_right_column();
    assert!(s.any_right_column_docked(true));
}

#[test]
fn right_column_docks_on_wide_without_live_session() {
    let mut s = IslandLayoutState::default();
    s.sync_breakpoint(1400.);
    assert!(s.any_right_column_docked(false));
    assert!(s.any_right_column_docked(true));
    s.sync_breakpoint(1000.);
    assert!(!s.any_right_column_docked(false));
}
