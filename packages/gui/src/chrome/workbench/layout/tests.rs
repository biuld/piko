//! Pure layout tree tests.

use super::island_tree::{IslandAxis, IslandNode};
use super::state::{LayoutBreakpoint, resolve_docked};
use super::*;

#[test]
fn breakpoint_from_docked() {
    assert_eq!(
        LayoutBreakpoint::from_docked(true, true),
        LayoutBreakpoint::Wide
    );
    assert_eq!(
        LayoutBreakpoint::from_docked(true, false),
        LayoutBreakpoint::Medium
    );
    assert_eq!(
        LayoutBreakpoint::from_docked(false, false),
        LayoutBreakpoint::Narrow
    );
    assert_eq!(
        LayoutBreakpoint::from_docked(false, true),
        LayoutBreakpoint::Narrow
    );
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
fn both_prefs_fit_both_columns() {
    // inner = 1400 - 16 = 1384; both_need = 620+220+260+16 = 1116
    let docked = resolve_docked(true, true, 1400.);
    assert!(docked.sessions);
    assert!(docked.right_column);
}

#[test]
fn both_prefs_collapse_right_first() {
    // left_need = 620+220+8 = 848; both_need = 1116
    // window 1000 → inner 984 → left only
    let docked = resolve_docked(true, true, 1000.);
    assert!(docked.sessions);
    assert!(!docked.right_column);
}

#[test]
fn both_prefs_collapse_left_when_too_narrow() {
    // window 800 → inner 784 < left_need 848 → neither
    let docked = resolve_docked(true, true, 800.);
    assert!(!docked.sessions);
    assert!(!docked.right_column);
}

#[test]
fn only_right_pref_docks_when_left_closed() {
    // right_need = 620+260+8 = 888; window 1000 → inner 984
    let docked = resolve_docked(false, true, 1000.);
    assert!(!docked.sessions);
    assert!(docked.right_column);
}

#[test]
fn auto_collapse_does_not_clear_prefs() {
    let mut s = IslandLayoutState::default();
    s.sync_window_width(800.);
    assert!(s.sessions_open);
    assert!(s.right_column_pref_open());
    assert!(!s.is_docked_visible(IslandId::Sessions, true));
    assert!(!s.any_right_column_docked(true));
    assert_eq!(s.breakpoint, LayoutBreakpoint::Narrow);

    s.sync_window_width(1400.);
    assert!(s.is_docked_visible(IslandId::Sessions, true));
    assert!(s.any_right_column_docked(true));
    assert_eq!(s.breakpoint, LayoutBreakpoint::Wide);
}

#[test]
fn medium_width_hides_right_keeps_sessions_pref() {
    let mut s = IslandLayoutState::default();
    s.sync_window_width(1000.);
    assert!(s.is_docked_visible(IslandId::Sessions, true));
    assert!(!s.is_docked_visible(IslandId::Agents, true));
    assert!(!s.any_right_column_docked(true));
    assert!(s.right_column_pref_open());
    s.sessions_open = false;
    s.sync_window_width(1000.);
    assert!(!s.is_docked_visible(IslandId::Sessions, true));
}

#[test]
fn wide_respects_right_column_pref() {
    let mut s = IslandLayoutState::default();
    s.sync_window_width(1400.);
    s.set_right_column_open(false);
    assert!(!s.any_right_column_docked(true));
    s.set_right_column_open(true);
    assert!(s.any_right_column_docked(true));
}

#[test]
fn right_column_docks_without_live_session() {
    let mut s = IslandLayoutState::default();
    s.sync_window_width(1400.);
    assert!(s.any_right_column_docked(false));
    assert!(s.any_right_column_docked(true));
    s.sync_window_width(1000.);
    assert!(!s.any_right_column_docked(false));
}

#[test]
fn toggle_pref_then_widen_restores_dock() {
    let mut s = IslandLayoutState::default();
    s.sync_window_width(1400.);
    s.set_right_column_open(false);
    assert!(!s.any_right_column_docked(true));
    s.set_right_column_open(true);
    s.sync_window_width(1000.);
    assert!(!s.any_right_column_docked(true));
    assert!(s.right_column_pref_open());
    s.sync_window_width(1400.);
    assert!(s.any_right_column_docked(true));
}

#[test]
fn window_min_matches_center_plus_gutters() {
    use super::state::{CENTER_MIN_WIDTH, WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH, WORKBENCH_GUTTER};
    assert_eq!(WINDOW_MIN_WIDTH, CENTER_MIN_WIDTH + 2.0 * WORKBENCH_GUTTER);
    assert_eq!(WINDOW_MIN_HEIGHT, 600.0);
}
