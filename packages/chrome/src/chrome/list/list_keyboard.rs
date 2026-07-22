//! List / tree **keyboard controller** (pure infrastructure).
//!
//! Apps own:
//! - the list data and length;
//! - mapping [`ListKeyEffect`] to product intents (open session, select agent…);
//! - GPUI `key_context` + keybindings (chrome defines recommended action names).
//!
//! Chrome owns:
//! - cursor index, wrap, clamp after data changes;
//! - interpretation of standard list key intents;
//! - which row should paint `keyboard_focused`.
//!
//! ## Recommended key map (app bindings)
//!
//! | Key | Intent |
//! |---|---|
//! | ↑ / ↓ | `Prev` / `Next` |
//! | Home / End | `Home` / `End` |
//! | Enter / Space | `Activate` |
//! | ← / → (trees) | `ToggleExpand` |
//!
//! Suggested `key_context`: island-specific (app-chosen) or shared
//! `ChromeList` if the island only hosts one list.

use super::list_nav::step_list_index;

/// Keyboard cursor for a flat visible list (tree already flattened by the app).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListKeyboard {
    cursor: Option<usize>,
}

/// Key-level intent (product-agnostic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKeyIntent {
    Prev,
    Next,
    Home,
    End,
    /// Enter / Space — activate the cursor row.
    Activate,
    /// Left/Right on trees — expand/collapse when the row has children.
    ToggleExpand,
}

/// Result of applying an intent (app maps to domain).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKeyEffect {
    None,
    /// Cursor moved (or ensured). `index` is always in `0..len` when `len > 0`.
    CursorMoved {
        index: usize,
    },
    Activate {
        index: usize,
    },
    ToggleExpand {
        index: usize,
    },
}

impl ListKeyboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    /// Whether row `index` should paint the keyboard focus ring.
    pub fn is_row_focused(&self, index: usize) -> bool {
        self.cursor == Some(index)
    }

    pub fn clear(&mut self) {
        self.cursor = None;
    }

    /// Force the cursor onto `index` (clamped to `0..len`). Clears when `len == 0`.
    ///
    /// Use when selection is set externally (click, domain apply) and the
    /// keyboard caret must follow.
    pub fn set_cursor(&mut self, len: usize, index: usize) {
        if len == 0 {
            self.cursor = None;
            return;
        }
        self.cursor = Some(index.min(len - 1));
    }

    /// Clamp cursor after the visible list length changes.
    pub fn sync_len(&mut self, len: usize) {
        if len == 0 {
            self.cursor = None;
            return;
        }
        if self.cursor.is_some_and(|ix| ix >= len) {
            self.cursor = Some(len - 1);
        }
    }

    /// When the list (or island) gains keyboard focus and cursor is empty, land on
    /// `preferred` if in range, else `0`.
    pub fn ensure_cursor(&mut self, len: usize, preferred: Option<usize>) -> Option<usize> {
        if len == 0 {
            self.cursor = None;
            return None;
        }
        if self.cursor.is_some() {
            self.sync_len(len);
            return self.cursor;
        }
        let ix = preferred.filter(|i| *i < len).unwrap_or(0);
        self.cursor = Some(ix);
        self.cursor
    }

    /// Apply a key intent. Always call [`sync_len`] (or pass current `len`) first
    /// if data may have changed since last use.
    pub fn apply(&mut self, len: usize, intent: ListKeyIntent) -> ListKeyEffect {
        self.sync_len(len);
        if len == 0 {
            return ListKeyEffect::None;
        }
        match intent {
            ListKeyIntent::Prev => {
                // No cursor yet → land on last; otherwise step.
                let ix = if self.cursor.is_none() {
                    len - 1
                } else {
                    step_list_index(len, self.cursor, -1).expect("len > 0")
                };
                self.cursor = Some(ix);
                ListKeyEffect::CursorMoved { index: ix }
            }
            ListKeyIntent::Next => {
                // No cursor yet → land on first; otherwise step.
                let ix = if self.cursor.is_none() {
                    0
                } else {
                    step_list_index(len, self.cursor, 1).expect("len > 0")
                };
                self.cursor = Some(ix);
                ListKeyEffect::CursorMoved { index: ix }
            }
            ListKeyIntent::Home => {
                self.cursor = Some(0);
                ListKeyEffect::CursorMoved { index: 0 }
            }
            ListKeyIntent::End => {
                let ix = len - 1;
                self.cursor = Some(ix);
                ListKeyEffect::CursorMoved { index: ix }
            }
            ListKeyIntent::Activate => match self.cursor {
                Some(index) if index < len => ListKeyEffect::Activate { index },
                _ => {
                    let ix = self.ensure_cursor(len, None).expect("len > 0");
                    ListKeyEffect::Activate { index: ix }
                }
            },
            ListKeyIntent::ToggleExpand => match self.cursor {
                Some(index) if index < len => ListKeyEffect::ToggleExpand { index },
                _ => {
                    let ix = self.ensure_cursor(len, None).expect("len > 0");
                    ListKeyEffect::ToggleExpand { index: ix }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_wraps_and_activates() {
        let mut kb = ListKeyboard::new();
        // First Next with empty cursor lands on 0.
        assert_eq!(
            kb.apply(3, ListKeyIntent::Next),
            ListKeyEffect::CursorMoved { index: 0 }
        );
        assert_eq!(
            kb.apply(3, ListKeyIntent::Next),
            ListKeyEffect::CursorMoved { index: 1 }
        );
        assert_eq!(
            kb.apply(3, ListKeyIntent::Next),
            ListKeyEffect::CursorMoved { index: 2 }
        );
        assert_eq!(
            kb.apply(3, ListKeyIntent::Next),
            ListKeyEffect::CursorMoved { index: 0 }
        );
        assert_eq!(
            kb.apply(3, ListKeyIntent::Activate),
            ListKeyEffect::Activate { index: 0 }
        );
        let mut kb2 = ListKeyboard::new();
        assert_eq!(
            kb2.apply(3, ListKeyIntent::Prev),
            ListKeyEffect::CursorMoved { index: 2 }
        );
    }

    #[test]
    fn sync_len_clamps() {
        let mut kb = ListKeyboard::new();
        kb.ensure_cursor(5, Some(4));
        kb.sync_len(2);
        assert_eq!(kb.cursor(), Some(1));
        kb.sync_len(0);
        assert_eq!(kb.cursor(), None);
    }

    #[test]
    fn empty_list_is_noop() {
        let mut kb = ListKeyboard::new();
        assert_eq!(kb.apply(0, ListKeyIntent::Next), ListKeyEffect::None);
        assert_eq!(kb.apply(0, ListKeyIntent::Activate), ListKeyEffect::None);
    }

    #[test]
    fn set_cursor_clamps_and_clears() {
        let mut kb = ListKeyboard::new();
        kb.set_cursor(5, 3);
        assert_eq!(kb.cursor(), Some(3));
        kb.set_cursor(5, 99);
        assert_eq!(kb.cursor(), Some(4));
        kb.set_cursor(0, 0);
        assert_eq!(kb.cursor(), None);
    }
}
