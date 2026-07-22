//! Keyboard focus ownership across workbench islands.
//!
//! Chrome ownership ([`FocusRing`]) is separate from keyboard placement.
//! The app host sets the ring / chrome border, then either:
//! - [`FocusReason::Activate`]: calls `take_keyboard_focus` on the island Entity
//!   (Tab, palette focus, overlay restore).
//! - [`FocusReason::Claimed`]: pointer path — island already focused a handle or
//!   input; host updates chrome only and must not steal window focus.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusCycleDir {
    Next,
    Prev,
}

/// Why chrome is handing keyboard focus to an island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusReason {
    /// Entered the island (Tab, command, restore). Island places the caret.
    Activate,
    /// Pointer already focused something inside the island; host must not steal.
    Claimed,
}

/// Successful focus ownership move (roadmap B3).
///
/// Returned by [`FocusRing::transfer`] and
/// [`super::contract::IslandFocusTable::try_focus`] so hosts can log, test, and
/// implement “already focused” policy without re-reading the ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTransition<Id: Copy + Eq> {
    pub from: Id,
    pub to: Id,
}

impl<Id: Copy + Eq> FocusTransition<Id> {
    /// True when ownership did not change (`from == to`).
    pub fn unchanged(self) -> bool
    where
        Id: PartialEq,
    {
        self.from == self.to
    }
}

/// Which island currently owns keyboard focus.
///
/// `Id` is app-defined (app-defined leaf id). Chrome never hard-codes product
/// islands.
#[derive(Debug, Clone)]
pub struct FocusRing<Id: Copy + Eq> {
    focused: Id,
    /// Restored after sheet/dialog dismissal.
    saved: Option<Id>,
}

impl<Id: Copy + Eq> FocusRing<Id> {
    pub fn new(initial: Id) -> Self {
        Self {
            focused: initial,
            saved: None,
        }
    }

    pub fn focused(&self) -> Id {
        self.focused
    }

    pub fn set_focused(&mut self, id: Id) {
        self.focused = id;
    }

    /// Move ownership to `to` and report the prior/next ids.
    pub fn transfer(&mut self, to: Id) -> FocusTransition<Id> {
        let from = self.focused;
        self.focused = to;
        FocusTransition { from, to }
    }

    pub fn save_and_focus(&mut self, id: Id) {
        self.saved = Some(self.focused);
        self.focused = id;
    }

    /// Remember the current island for restore when leaving a secondary surface.
    pub fn save_for_restore(&mut self) {
        if self.saved.is_none() {
            self.saved = Some(self.focused);
        }
    }

    pub fn restore(&mut self) {
        if let Some(id) = self.saved.take() {
            self.focused = id;
        }
    }

    /// Cycle among currently visible islands.
    pub fn cycle(&mut self, dir: FocusCycleDir, visible: &[Id]) {
        if visible.is_empty() {
            return;
        }
        let Some(ix) = visible.iter().position(|id| *id == self.focused) else {
            self.focused = visible[0];
            return;
        };
        let next = match dir {
            FocusCycleDir::Next => (ix + 1) % visible.len(),
            FocusCycleDir::Prev => {
                if ix == 0 {
                    visible.len() - 1
                } else {
                    ix - 1
                }
            }
        };
        self.focused = visible[next];
    }
}

#[cfg(test)]
mod tests {
    // Avoid `use super::*` — keeps GPUI out of #[test] expansion.
    use super::{FocusCycleDir, FocusRing, FocusTransition};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestId {
        A,
        B,
        C,
    }

    #[test]
    fn cycle_wraps_among_visible() {
        let mut ring = FocusRing::new(TestId::A);
        let visible = vec![TestId::A, TestId::B, TestId::C];
        ring.cycle(FocusCycleDir::Next, &visible);
        assert_eq!(ring.focused(), TestId::B);
        ring.cycle(FocusCycleDir::Next, &visible);
        assert_eq!(ring.focused(), TestId::C);
        ring.cycle(FocusCycleDir::Next, &visible);
        assert_eq!(ring.focused(), TestId::A);
        ring.cycle(FocusCycleDir::Prev, &visible);
        assert_eq!(ring.focused(), TestId::C);
    }

    #[test]
    fn transfer_reports_from_to() {
        let mut ring = FocusRing::new(TestId::A);
        let t = ring.transfer(TestId::B);
        assert_eq!(
            t,
            FocusTransition {
                from: TestId::A,
                to: TestId::B,
            }
        );
        assert!(!t.unchanged());
        assert_eq!(ring.focused(), TestId::B);

        let same = ring.transfer(TestId::B);
        assert!(same.unchanged());
        assert_eq!(same.from, TestId::B);
        assert_eq!(same.to, TestId::B);
    }
}
