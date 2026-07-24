//! Product archipelago ids and workspace declarations for piko-gui.
//!
//! Navigation: [`piko_chrome::runtime::archipelago::ArchipelagoRouter`] via
//! [`crate::app::wiring::archipelago_nav`] (`route_archipelago_nav`).
//! Settings *section* is app state beside the router.
//!
//! Body workspaces are the source of truth for island trees and Tab focus
//! order (roadmap A2). Shell assemble and focus cycle read these declarations.

use piko_chrome::runtime::archipelago::{ArchipelagoRouter, ArchipelagoWorkspace};
use piko_chrome::runtime::layout::{IslandAxis, IslandNode};

use crate::features::{SETTINGS_FOCUS_ORDER, SettingsIslandId};
use crate::shell::workbench::{IslandId, workbench_island_tree};

/// Exclusive product places (each body is an island workspace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ArchipelagoId {
    #[default]
    Workbench,
    Settings,
}

/// Router + helpers used by `DesktopApp`.
pub type AppArchipelago = ArchipelagoRouter<ArchipelagoId>;

/// Keyboard focus target captured when the first overlay layer opens.
///
/// Keeping the archipelago in the snapshot prevents an overlay opened from
/// Settings from restoring focus into a hidden Workbench island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchipelagoFocusTarget {
    Workbench(IslandId),
    Settings(SettingsIslandId),
}

impl ArchipelagoFocusTarget {
    pub fn capture(active: ArchipelagoId, workbench: IslandId, settings: SettingsIslandId) -> Self {
        match active {
            ArchipelagoId::Workbench => Self::Workbench(workbench),
            ArchipelagoId::Settings => Self::Settings(settings),
        }
    }
}

#[cfg(test)]
pub fn is_workbench(router: &AppArchipelago) -> bool {
    router.is_active(ArchipelagoId::Workbench)
}

/// Workbench body: Sessions | (Timeline/Composer) | (Agents/Tree).
///
/// `island_tree` + `focus_order` are what assemble and Tab cycle consult.
pub fn workbench_workspace() -> ArchipelagoWorkspace<ArchipelagoId, IslandId> {
    ArchipelagoWorkspace::new(
        ArchipelagoId::Workbench,
        workbench_island_tree(),
        crate::shell::workbench::ALL_ISLAND_IDS,
    )
}

/// Settings body: Nav | Panel (horizontal split).
///
/// Tab order is this workspace's `focus_order` (not a parallel constant path).
pub fn settings_workspace() -> ArchipelagoWorkspace<ArchipelagoId, SettingsIslandId> {
    let tree = IslandNode::split(
        IslandAxis::Horizontal,
        [
            IslandNode::island(SettingsIslandId::Nav),
            IslandNode::island(SettingsIslandId::Panel),
        ],
    );
    ArchipelagoWorkspace::new(ArchipelagoId::Settings, tree, SETTINGS_FOCUS_ORDER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_chrome::runtime::layout::IslandNode;

    #[test]
    fn default_router_is_workbench() {
        let r = AppArchipelago::default();
        assert!(is_workbench(&r));
    }

    #[test]
    fn enter_settings_and_leave() {
        let mut r = AppArchipelago::new(ArchipelagoId::Workbench);
        assert!(r.enter(ArchipelagoId::Settings).changed());
        assert!(r.is_active(ArchipelagoId::Settings));
        assert!(r.leave().changed());
        assert!(is_workbench(&r));
    }

    #[test]
    fn route_archipelago_nav_toggle_pair() {
        use piko_chrome::runtime::archipelago::{ArchipelagoNav, route_archipelago_nav};

        let mut r = AppArchipelago::new(ArchipelagoId::Workbench);
        let t = route_archipelago_nav(
            &mut r,
            ArchipelagoNav::TogglePair {
                a: ArchipelagoId::Workbench,
                b: ArchipelagoId::Settings,
            },
        );
        assert!(t.changed());
        assert!(r.is_active(ArchipelagoId::Settings));
        let t = route_archipelago_nav(
            &mut r,
            ArchipelagoNav::TogglePair {
                a: ArchipelagoId::Workbench,
                b: ArchipelagoId::Settings,
            },
        );
        assert!(t.changed());
        assert!(is_workbench(&r));
    }

    #[test]
    fn settings_workspace_is_nav_panel_split() {
        let ws = settings_workspace();
        assert_eq!(ws.id, ArchipelagoId::Settings);
        assert_eq!(ws.focus_order.as_slice(), SETTINGS_FOCUS_ORDER.as_slice());
        match &ws.island_tree {
            IslandNode::Split {
                axis: IslandAxis::Horizontal,
                children,
            } => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0], IslandNode::island(SettingsIslandId::Nav));
                assert_eq!(children[1], IslandNode::island(SettingsIslandId::Panel));
            }
            other => panic!("expected horizontal split, got {other:?}"),
        }
    }

    #[test]
    fn workbench_workspace_matches_focus_order() {
        let ws = workbench_workspace();
        assert_eq!(ws.id, ArchipelagoId::Workbench);
        assert_eq!(
            ws.focus_order.as_slice(),
            crate::shell::workbench::ALL_ISLAND_IDS.as_slice()
        );
    }

    #[test]
    fn overlay_focus_target_tracks_opening_archipelago() {
        assert_eq!(
            ArchipelagoFocusTarget::capture(
                ArchipelagoId::Workbench,
                IslandId::Composer,
                SettingsIslandId::Panel,
            ),
            ArchipelagoFocusTarget::Workbench(IslandId::Composer)
        );
        assert_eq!(
            ArchipelagoFocusTarget::capture(
                ArchipelagoId::Settings,
                IslandId::Composer,
                SettingsIslandId::Panel,
            ),
            ArchipelagoFocusTarget::Settings(SettingsIslandId::Panel)
        );
    }
}
