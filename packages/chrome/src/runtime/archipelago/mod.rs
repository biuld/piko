//! Archipelago: exclusive full-frame workspace of islands + navigation routing.
//!
//! An **Archipelago** is where the user *is* — TitleBar + body + optional
//! StatusBar, switched as one unit. The body is an **island workspace**
//! ([`IslandNode`]). Overlays paint above any archipelago.
//!
//! ```text
//! Window
//! ├── ArchipelagoRouter  (which archipelago is exclusive)
//! │   └── active body = IslandNode tree + IslandFocusTable
//! └── Overlay (orthogonal)
//! ```
//!
//! See `docs/design/archipelago.md` and `docs/features/archipelago.md`.

use std::fmt::Debug;
use std::hash::Hash;

use gpui::*;

use crate::runtime::island::{
    FocusMsg, FocusReason, FocusRing, IslandFocusTable, route_focus_message,
};
use crate::runtime::layout::IslandNode;

// ── Archipelago identity & workspace ───────────────────────────────────────

/// One archipelago’s island workspace (body layout).
///
/// `ArchipelagoId` / `IslandId` are app-defined. Chrome does not know Main
/// or Prefs — only that an archipelago owns a split tree of islands.
#[derive(Debug, Clone)]
pub struct ArchipelagoWorkspace<ArchipelagoId, IslandId: Copy + Eq> {
    pub id: ArchipelagoId,
    /// Default island split tree before visibility prune.
    pub island_tree: IslandNode<IslandId>,
    /// Islands that may own keyboard chrome focus while this archipelago is
    /// active (stable Tab order when all visible).
    pub focus_order: Vec<IslandId>,
}

impl<ArchipelagoId, IslandId: Copy + Eq> ArchipelagoWorkspace<ArchipelagoId, IslandId> {
    pub fn new(
        id: ArchipelagoId,
        island_tree: IslandNode<IslandId>,
        focus_order: impl IntoIterator<Item = IslandId>,
    ) -> Self {
        Self {
            id,
            island_tree,
            focus_order: focus_order.into_iter().collect(),
        }
    }
}

// ── Router (exclusive archipelago) ─────────────────────────────────────────

/// Exclusive Archipelago navigator.
///
/// Only one archipelago is active. [`enter`](Self::enter) remembers the
/// previous archipelago for [`leave`](Self::leave) (e.g. Main → Preferences →
/// Escape). [`go`](Self::go) hard-cuts without a restore stack.
#[derive(Debug, Clone)]
pub struct ArchipelagoRouter<ArchipelagoId: Copy + Eq> {
    active: ArchipelagoId,
    saved: Option<ArchipelagoId>,
}

/// Result of a router mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchipelagoTransition<ArchipelagoId: Copy + Eq> {
    /// Active id (and restore stack side-effects that matter to the frame) unchanged.
    Unchanged { active: ArchipelagoId },
    /// Active archipelago switched.
    Changed {
        from: ArchipelagoId,
        to: ArchipelagoId,
    },
}

impl<ArchipelagoId: Copy + Eq> ArchipelagoTransition<ArchipelagoId> {
    pub fn changed(self) -> bool {
        matches!(self, Self::Changed { .. })
    }
}

impl<ArchipelagoId: Copy + Eq> ArchipelagoRouter<ArchipelagoId> {
    pub fn new(initial: ArchipelagoId) -> Self {
        Self {
            active: initial,
            saved: None,
        }
    }

    pub fn active(&self) -> ArchipelagoId {
        self.active
    }

    pub fn is_active(&self, id: ArchipelagoId) -> bool {
        self.active == id
    }

    pub fn saved(&self) -> Option<ArchipelagoId> {
        self.saved
    }

    /// Hard-cut to `id` (clears restore).
    pub fn go(&mut self, id: ArchipelagoId) -> ArchipelagoTransition<ArchipelagoId> {
        let from = self.active;
        self.saved = None;
        if from == id {
            return ArchipelagoTransition::Unchanged { active: id };
        }
        self.active = id;
        ArchipelagoTransition::Changed { from, to: id }
    }

    /// Switch to `id`, remembering the current archipelago for [`Self::leave`].
    pub fn enter(&mut self, id: ArchipelagoId) -> ArchipelagoTransition<ArchipelagoId> {
        if self.active == id {
            return ArchipelagoTransition::Unchanged { active: id };
        }
        let from = self.active;
        if self.saved.is_none() {
            self.saved = Some(from);
        }
        self.active = id;
        ArchipelagoTransition::Changed { from, to: id }
    }

    /// Restore the archipelago saved by [`Self::enter`].
    pub fn leave(&mut self) -> ArchipelagoTransition<ArchipelagoId> {
        match self.saved.take() {
            Some(prev) => {
                let from = self.active;
                self.active = prev;
                ArchipelagoTransition::Changed { from, to: prev }
            }
            None => ArchipelagoTransition::Unchanged {
                active: self.active,
            },
        }
    }

    /// Toggle between `a` and `b` (enter/leave style).
    pub fn toggle_pair(
        &mut self,
        a: ArchipelagoId,
        b: ArchipelagoId,
    ) -> ArchipelagoTransition<ArchipelagoId> {
        if self.active == a {
            self.enter(b)
        } else if self.active == b {
            let t = self.leave();
            if t.changed() { t } else { self.go(a) }
        } else {
            self.enter(b)
        }
    }
}

impl<ArchipelagoId: Copy + Eq + Default> Default for ArchipelagoRouter<ArchipelagoId> {
    fn default() -> Self {
        Self::new(ArchipelagoId::default())
    }
}

// ── Archipelago-level navigation messages ──────────────────────────────────

/// Chrome-owned archipelago navigation intents (product enums map via
/// [`ArchipelagoMessage`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchipelagoNav<ArchipelagoId: Copy + Eq> {
    /// Hard-cut; clear restore stack.
    Go { id: ArchipelagoId },
    /// Enter peer archipelago; save current for leave.
    Enter { id: ArchipelagoId },
    /// Restore saved archipelago (Escape / dismiss).
    Leave,
    /// Toggle between two peers (e.g. Main ↔ Preferences).
    TogglePair { a: ArchipelagoId, b: ArchipelagoId },
}

/// Unified chrome route: archipelago navigation **or** island focus.
///
/// Host dispatch should try this **before** product domain arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeRoute<ArchipelagoId: Copy + Eq, IslandId: Copy + Eq> {
    Archipelago(ArchipelagoNav<ArchipelagoId>),
    Island(FocusMsg<IslandId>),
}

/// Product message enums that may carry archipelago or island chrome intents.
pub trait ArchipelagoMessage {
    type ArchipelagoId: Copy + Eq + 'static;
    type IslandId: Copy + Eq + Hash + 'static;

    fn as_chrome_route(&self) -> Option<ChromeRoute<Self::ArchipelagoId, Self::IslandId>>;
}

/// Apply an archipelago navigation intent.
pub fn route_archipelago_nav<ArchipelagoId: Copy + Eq>(
    router: &mut ArchipelagoRouter<ArchipelagoId>,
    nav: ArchipelagoNav<ArchipelagoId>,
) -> ArchipelagoTransition<ArchipelagoId> {
    match nav {
        ArchipelagoNav::Go { id } => router.go(id),
        ArchipelagoNav::Enter { id } => router.enter(id),
        ArchipelagoNav::Leave => router.leave(),
        ArchipelagoNav::TogglePair { a, b } => router.toggle_pair(a, b),
    }
}

/// Outcome of [`route_chrome_message`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeRouteOutcome<ArchipelagoId: Copy + Eq = ()> {
    /// Archipelago router mutation result (may be [`ArchipelagoTransition::Unchanged`]).
    Archipelago(ArchipelagoTransition<ArchipelagoId>),
    /// Island focus handoff applied.
    IslandFocus,
    /// Island focus target was not registered; ring left unchanged.
    IslandUnknown,
}

/// Route a chrome-level message: archipelago nav first, then island focus.
///
/// Island focus is only applied when the host decides the active archipelago
/// owns that island table. Pass `island_from` for [`FocusMsg::ClaimFocus`].
pub fn route_chrome_message<ArchipelagoId, IslandId>(
    router: &mut ArchipelagoRouter<ArchipelagoId>,
    island_table: &IslandFocusTable<IslandId>,
    island_ring: &mut FocusRing<IslandId>,
    island_from: IslandId,
    route: ChromeRoute<ArchipelagoId, IslandId>,
    window: &mut Window,
    cx: &mut App,
) -> ChromeRouteOutcome<ArchipelagoId>
where
    ArchipelagoId: Copy + Eq,
    IslandId: Copy + Eq + Hash,
{
    match route {
        ChromeRoute::Archipelago(nav) => {
            ChromeRouteOutcome::Archipelago(route_archipelago_nav(router, nav))
        }
        ChromeRoute::Island(focus) => {
            match route_focus_message(island_table, island_ring, island_from, focus, window, cx) {
                Ok(_transition) => ChromeRouteOutcome::IslandFocus,
                Err(_) => ChromeRouteOutcome::IslandUnknown,
            }
        }
    }
}

/// When the archipelago changes, re-apply island chrome for the workspace that
/// is now active (app picks which table/ring belong to the active archipelago).
pub fn activate_archipelago_islands<IslandId: Copy + Eq + Hash>(
    table: &IslandFocusTable<IslandId>,
    ring: &mut FocusRing<IslandId>,
    reason: FocusReason,
    window: &mut Window,
    cx: &mut App,
) {
    let id = ring.focused();
    table.focus(ring, id, reason, window, cx);
}

#[cfg(test)]
mod router_tests {
    // Avoid `use super::*` — pulls GPUI into #[test] expansion (recursion limit).
    use super::{
        ArchipelagoMessage, ArchipelagoNav, ArchipelagoRouter, ArchipelagoWorkspace, ChromeRoute,
        FocusMsg, IslandNode,
    };
    use crate::runtime::layout::IslandAxis;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    enum Arch {
        #[default]
        Main,
        Prefs,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Isle {
        Main,
        Side,
    }

    use super::ArchipelagoTransition;

    #[test]
    fn enter_leave_restores() {
        let mut r = ArchipelagoRouter::new(Arch::Main);
        assert!(r.enter(Arch::Prefs).changed());
        assert!(r.is_active(Arch::Prefs));
        assert_eq!(r.saved(), Some(Arch::Main));
        assert!(r.leave().changed());
        assert!(r.is_active(Arch::Main));
        assert!(!r.leave().changed());
    }

    #[test]
    fn go_clears_saved() {
        let mut r = ArchipelagoRouter::new(Arch::Main);
        r.enter(Arch::Prefs);
        assert!(matches!(
            r.go(Arch::Main),
            ArchipelagoTransition::Changed {
                from: Arch::Prefs,
                to: Arch::Main
            }
        ));
        assert!(r.saved().is_none());
        assert!(!r.leave().changed());
    }

    #[test]
    fn enter_same_is_unchanged() {
        let mut r = ArchipelagoRouter::new(Arch::Main);
        assert!(!r.enter(Arch::Main).changed());
    }

    #[test]
    fn toggle_pair_workbench_settings() {
        let mut r = ArchipelagoRouter::new(Arch::Main);
        assert!(r.toggle_pair(Arch::Main, Arch::Prefs).changed());
        assert!(r.is_active(Arch::Prefs));
        assert!(r.toggle_pair(Arch::Main, Arch::Prefs).changed());
        assert!(r.is_active(Arch::Main));
    }

    #[test]
    fn workspace_holds_tree_and_focus_order() {
        let tree = IslandNode::split(
            IslandAxis::Horizontal,
            [
                IslandNode::island(Isle::Side),
                IslandNode::island(Isle::Main),
            ],
        );
        let ws = ArchipelagoWorkspace::new(Arch::Main, tree, [Isle::Side, Isle::Main]);
        assert_eq!(ws.focus_order, vec![Isle::Side, Isle::Main]);
    }

    #[derive(Debug)]
    enum AppMsg {
        OpenPrefs,
        ClaimFocus,
        Domain,
    }

    impl ArchipelagoMessage for AppMsg {
        type ArchipelagoId = Arch;
        type IslandId = Isle;

        fn as_chrome_route(&self) -> Option<ChromeRoute<Arch, Isle>> {
            match self {
                AppMsg::OpenPrefs => Some(ChromeRoute::Archipelago(ArchipelagoNav::Enter {
                    id: Arch::Prefs,
                })),
                AppMsg::ClaimFocus => Some(ChromeRoute::Island(FocusMsg::ClaimFocus)),
                AppMsg::Domain => None,
            }
        }
    }

    #[test]
    fn archipelago_message_classifies_routes() {
        assert!(matches!(
            AppMsg::OpenPrefs.as_chrome_route(),
            Some(ChromeRoute::Archipelago(ArchipelagoNav::Enter {
                id: Arch::Prefs
            }))
        ));
        assert!(matches!(
            AppMsg::ClaimFocus.as_chrome_route(),
            Some(ChromeRoute::Island(FocusMsg::ClaimFocus))
        ));
        assert!(AppMsg::Domain.as_chrome_route().is_none());
    }
}
