//! Island runtime contracts enforced by the chrome kit.
//!
//! Apps implement these traits; chrome provides routing tables, focus-message
//! classification, and deferred dispatch so islands stay isolated and
//! host-mediated.
//!
//! ```text
//! IslandView ──schedule_island_message──► IslandHost
//!      ▲                                      │
//!      │    IslandMessage::as_focus_msg?      │
//!      │              │                       │
//!      │              ▼                       │
//!      │     route_focus_message              │
//!      │    IslandFocusTable + FocusRing      │
//!      └──── set_chrome_focused / handoff ────┘
//! ```

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use gpui::*;

use super::focus::{FocusReason, FocusRing, FocusTransition};

// ── Island view (feature Entity) ───────────────────────────────────────────

/// Contract every workbench island Entity must satisfy.
///
/// Product projections (`apply`) and domain messages stay in the app; chrome
/// only standardizes **focus chrome** and **keyboard handoff** so the host can
/// route without matching on concrete island types.
pub trait IslandView: 'static + Sized {
    /// App-defined leaf id (`IslandId`, `PaneId`, …).
    type Id: Copy + Eq + Hash + 'static;

    /// Paint or clear the island focus ring (chrome ownership, not caret).
    fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>);

    /// Place (or refuse to steal) keyboard focus for [`FocusReason`].
    ///
    /// - [`FocusReason::Activate`]: host entered this island (Tab, restore, …).
    /// - [`FocusReason::Claimed`]: pointer already focused an inner control;
    ///   default implementations must not steal from that control.
    fn take_keyboard_focus(
        &mut self,
        reason: FocusReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
}

/// Default Activate handoff: focus the island's outer [`FocusHandle`].
///
/// Claimed is a no-op (caller already placed focus). Islands with inner inputs
/// (islands with inner Inputs) override [`IslandView::take_keyboard_focus`].
pub fn activate_focus_handle(handle: &FocusHandle, reason: FocusReason, window: &mut Window) {
    if reason == FocusReason::Activate {
        window.focus(handle);
    }
}

// ── Focus messages (chrome-owned layer) ────────────────────────────────────

/// Focus-related intents that chrome knows how to route.
///
/// Product message enums live in the app but **must** expose these via
/// [`IslandMessage::as_focus_msg`] so hosts can handle focus without matching
/// product-specific variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMsg<Id: Copy + Eq> {
    /// Emitter already placed keyboard focus; sync chrome ring only (Claimed).
    ClaimFocus,
    /// Host should Activate another (or the same) island.
    FocusIsland { id: Id },
}

/// App message types that may carry chrome focus intents.
///
/// Implement on the product `IslandMsg` (or equivalent). Non-focus variants
/// return `None` and are handled by the app host.
pub trait IslandMessage {
    type Id: Copy + Eq + Hash + 'static;

    /// When `Some`, the host should call [`route_focus_message`] and skip
    /// product dispatch for this delivery.
    fn as_focus_msg(&self) -> Option<FocusMsg<Self::Id>>;
}

/// Apply a chrome [`FocusMsg`] through the focus table + ring.
pub fn route_focus_message<Id: Copy + Eq + Hash>(
    table: &IslandFocusTable<Id>,
    ring: &mut FocusRing<Id>,
    from: Id,
    msg: FocusMsg<Id>,
    window: &mut Window,
    cx: &mut App,
) -> Result<FocusTransition<Id>, UnknownIsland> {
    match msg {
        FocusMsg::ClaimFocus => table.try_focus(ring, from, FocusReason::Claimed, window, cx),
        FocusMsg::FocusIsland { id } => {
            table.try_focus(ring, id, FocusReason::Activate, window, cx)
        }
    }
}

/// Target id was not registered in the focus table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownIsland;

// ── Heterogeneous focus table ──────────────────────────────────────────────

/// Object-safe slot so a host can store many typed island Entities in one table.
pub trait IslandFocusSlot {
    fn set_chrome_focused(&self, focused: bool, cx: &mut App);
    fn take_keyboard_focus(&self, reason: FocusReason, window: &mut Window, cx: &mut App);
}

impl<V> IslandFocusSlot for Entity<V>
where
    V: IslandView,
{
    fn set_chrome_focused(&self, focused: bool, cx: &mut App) {
        self.update(cx, |view, cx| {
            view.set_chrome_focused(focused, cx);
        });
    }

    fn take_keyboard_focus(&self, reason: FocusReason, window: &mut Window, cx: &mut App) {
        self.update(cx, |view, cx| {
            view.take_keyboard_focus(reason, window, cx);
        });
    }
}

/// Registry of island Entities for focus chrome + keyboard handoff.
///
/// Replaces N-way matches on concrete island Entities in the host.
#[derive(Default)]
pub struct IslandFocusTable<Id: Copy + Eq + Hash> {
    slots: HashMap<Id, Box<dyn IslandFocusSlot>>,
}

impl<Id: Copy + Eq + Hash> IslandFocusTable<Id> {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    /// Register (or replace) the Entity that backs `id`.
    pub fn register<V>(&mut self, id: Id, entity: Entity<V>)
    where
        V: IslandView<Id = Id>,
    {
        self.register_slot(id, Box::new(entity));
    }

    /// Register a pre-boxed slot (tests / non-Entity adapters).
    pub fn register_slot(&mut self, id: Id, slot: Box<dyn IslandFocusSlot>) {
        self.slots.insert(id, slot);
    }

    pub fn contains(&self, id: Id) -> bool {
        self.slots.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Panic if any expected id is missing, duplicated, or the table has extras.
    ///
    /// Call after registration (e.g. `DesktopApp::new`) so a new island that
    /// is focusable but not registered fails fast.
    pub fn assert_covers(&self, expected: &[Id])
    where
        Id: Debug,
    {
        let mut uniq = std::collections::HashSet::new();
        for id in expected {
            assert!(
                uniq.insert(*id),
                "IslandFocusTable expected list has duplicate {id:?}",
            );
        }
        assert_eq!(
            self.slots.len(),
            uniq.len(),
            "IslandFocusTable len {} != unique expected {} ({expected:?})",
            self.slots.len(),
            uniq.len(),
        );
        for id in &uniq {
            assert!(
                self.slots.contains_key(id),
                "IslandFocusTable missing registered island {id:?}",
            );
        }
        for id in self.slots.keys() {
            assert!(
                uniq.contains(id),
                "IslandFocusTable has extra registered island {id:?}",
            );
        }
    }

    /// Sync focus-ring flags: only `focused` is true, all others false.
    pub fn apply_chrome_rings(&self, focused: Id, cx: &mut App) {
        for (id, slot) in &self.slots {
            slot.set_chrome_focused(*id == focused, cx);
        }
    }

    /// Keyboard handoff only (caller already updated the ring if needed).
    pub fn handoff(&self, id: Id, reason: FocusReason, window: &mut Window, cx: &mut App) {
        if let Some(slot) = self.slots.get(&id) {
            slot.take_keyboard_focus(reason, window, cx);
        }
    }

    /// Validate + update ring ownership only (no paint / keyboard handoff).
    ///
    /// Unit-testable without a GPUI window. Fails without mutating the ring
    /// when `id` is not registered.
    pub fn claim_focus_id(
        &self,
        ring: &mut FocusRing<Id>,
        id: Id,
    ) -> Result<FocusTransition<Id>, UnknownIsland> {
        if !self.slots.contains_key(&id) {
            return Err(UnknownIsland);
        }
        Ok(ring.transfer(id))
    }

    /// Set ring ownership, paint rings, then Activate/Claimed handoff.
    ///
    /// Fails without mutating the ring when `id` is not registered.
    /// On success returns [`FocusTransition`] with prior and next focus ids.
    pub fn try_focus(
        &self,
        ring: &mut FocusRing<Id>,
        id: Id,
        reason: FocusReason,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<FocusTransition<Id>, UnknownIsland> {
        let transition = self.claim_focus_id(ring, id)?;
        self.apply_chrome_rings(id, cx);
        self.handoff(id, reason, window, cx);
        Ok(transition)
    }

    /// Like [`try_focus`], but `debug_assert`s on unknown ids (no ring mutation).
    pub fn focus(
        &self,
        ring: &mut FocusRing<Id>,
        id: Id,
        reason: FocusReason,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Err(UnknownIsland) = self.try_focus(ring, id, reason, window, cx) {
            debug_assert!(false, "IslandFocusTable::focus unknown island id");
        }
    }
}

// ── Host message sink ──────────────────────────────────────────────────────

/// Composition root that receives deferred messages from islands.
///
/// Islands must not hold sibling Entities for mutation; they only emit `Msg`
/// through [`schedule_island_message`].
///
/// When `Msg: IslandMessage`, handle focus first via [`route_focus_message`].
pub trait IslandHost: 'static + Sized {
    type Id: Copy + Eq + 'static;
    type Msg: 'static;

    fn handle_island_message(
        &mut self,
        from: Self::Id,
        msg: Self::Msg,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
}

/// Deliver an island message after the current GPUI effect cycle.
///
/// Island handlers run inside that island's `Entity::update`. Host handlers
/// often update the same island again (focus chrome, dirty push), which panics
/// if done synchronously. Deferring drops the island off the update stack first.
pub fn schedule_island_message<H>(
    host: WeakEntity<H>,
    from: H::Id,
    msg: H::Msg,
    window: &Window,
    cx: &mut App,
) where
    H: IslandHost,
{
    window.defer(cx, move |window, cx| {
        if let Some(host) = host.upgrade() {
            host.update(cx, |app, cx| {
                app.handle_island_message(from, msg, window, cx);
            });
        }
    });
}

#[cfg(test)]
mod message_tests {
    // Intentionally avoid `use super::*` (pulls gpui Entity/Window into the
    // test module and can hit #[test] recursion limits).
    use super::{FocusMsg, IslandMessage};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum TestId {
        B,
    }

    #[derive(Debug)]
    enum TestMsg {
        ClaimFocus,
        FocusIsland { id: TestId },
        Product,
    }

    impl IslandMessage for TestMsg {
        type Id = TestId;

        fn as_focus_msg(&self) -> Option<FocusMsg<TestId>> {
            match self {
                TestMsg::ClaimFocus => Some(FocusMsg::ClaimFocus),
                TestMsg::FocusIsland { id } => Some(FocusMsg::FocusIsland { id: *id }),
                TestMsg::Product => None,
            }
        }
    }

    #[test]
    fn island_message_classifies_focus_layer() {
        assert_eq!(
            TestMsg::ClaimFocus.as_focus_msg(),
            Some(FocusMsg::ClaimFocus)
        );
        assert_eq!(
            TestMsg::FocusIsland { id: TestId::B }.as_focus_msg(),
            Some(FocusMsg::FocusIsland { id: TestId::B })
        );
        assert_eq!(TestMsg::Product.as_focus_msg(), None);
    }
}

#[cfg(test)]
mod claim_focus_tests {
    // Pure table + ring validation (no Window/App). Success from→to is covered
    // by `FocusRing::transfer` unit tests; try_focus composes claim + paint.
    use super::{FocusRing, IslandFocusTable, UnknownIsland};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum TestId {
        A,
        B,
    }

    #[test]
    fn claim_focus_unknown_leaves_ring_intact() {
        let table = IslandFocusTable::<TestId>::new();
        let mut ring = FocusRing::new(TestId::A);
        assert_eq!(
            table.claim_focus_id(&mut ring, TestId::B),
            Err(UnknownIsland)
        );
        assert_eq!(ring.focused(), TestId::A);
    }
}
