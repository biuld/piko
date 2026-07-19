//! Keyboard focus ownership across Workbench islands.
//!
//! `IslandFocusRing::set_focused` is driven today by
//! `DesktopApp::dispatch_island_msg`; cycling / restore land once Tab
//! navigation and sheet dismissal are wired to the island Entities.

use crate::chrome::workbench::IslandId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusCycleDir {
    Next,
    Prev,
}

/// Which island currently owns keyboard focus.
#[derive(Debug, Clone)]
pub struct IslandFocusRing {
    focused: IslandId,
    /// Restored after sheet/dialog dismissal.
    saved: Option<IslandId>,
}

impl Default for IslandFocusRing {
    fn default() -> Self {
        Self {
            focused: IslandId::Sessions,
            saved: None,
        }
    }
}

impl IslandFocusRing {
    pub fn focused(&self) -> IslandId {
        self.focused
    }

    pub fn set_focused(&mut self, id: IslandId) {
        self.focused = id;
    }

    pub fn save_and_focus(&mut self, id: IslandId) {
        self.saved = Some(self.focused);
        self.focused = id;
    }

    /// Remember the current island for restore when leaving Settings.
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

    /// Cycle among currently visible islands. Timeline and Composer always
    /// count as visible in the center column.
    pub fn cycle(&mut self, dir: FocusCycleDir, visible: &[IslandId]) {
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

/// Stable Tab order for visible docks.
pub fn focus_order(visible: impl Fn(IslandId) -> bool) -> Vec<IslandId> {
    [
        IslandId::Sessions,
        IslandId::Timeline,
        IslandId::Composer,
        IslandId::Agents,
        IslandId::Tree,
    ]
    .into_iter()
    .filter(|id| visible(*id))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_wraps_among_visible() {
        let mut ring = IslandFocusRing::default();
        ring.set_focused(IslandId::Sessions);
        let visible = vec![IslandId::Sessions, IslandId::Timeline, IslandId::Composer];
        ring.cycle(FocusCycleDir::Next, &visible);
        assert_eq!(ring.focused(), IslandId::Timeline);
        ring.cycle(FocusCycleDir::Next, &visible);
        assert_eq!(ring.focused(), IslandId::Composer);
        ring.cycle(FocusCycleDir::Next, &visible);
        assert_eq!(ring.focused(), IslandId::Sessions);
        ring.cycle(FocusCycleDir::Prev, &visible);
        assert_eq!(ring.focused(), IslandId::Composer);
    }
}
