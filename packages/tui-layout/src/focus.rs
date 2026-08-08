//! LIFO focus stack — targets are client-defined (`T`).

use std::time::Instant;

/// Generic focus stack. `T` is the client focus target (enum, ids, …).
///
/// Stack bottom is always `base` (e.g. product “editor” or idle target).
pub struct FocusManager<T> {
    base: T,
    stack: Vec<T>,
    pub last_esc_pressed: Option<Instant>,
}

impl<T: Copy + Eq> FocusManager<T> {
    /// Create a manager whose stack bottom is always `base`.
    pub fn new(base: T) -> Self {
        Self {
            base,
            stack: vec![base],
            last_esc_pressed: None,
        }
    }

    pub fn base(&self) -> T {
        self.base
    }

    pub fn active(&self) -> T {
        self.stack.last().copied().unwrap_or(self.base)
    }

    /// Legacy alias for [`Self::active`].
    pub fn active_mode(&self) -> T {
        self.active()
    }

    pub fn push(&mut self, target: T) {
        if self.stack.last() != Some(&target) {
            self.stack.push(target);
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.stack.len() > 1 {
            self.stack.pop()
        } else {
            None
        }
    }

    pub fn clear_to_base(&mut self) {
        self.stack.clear();
        self.stack.push(self.base);
    }

    /// Legacy name for [`Self::clear_to_base`].
    pub fn clear_to_chat(&mut self) {
        self.clear_to_base();
    }

    pub fn is_base_active(&self) -> bool {
        self.active() == self.base
    }

    /// True when something above the base owns focus.
    pub fn is_blocking_surface_active(&self) -> bool {
        !self.is_base_active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum F {
        Base,
        A,
        B,
    }

    #[test]
    fn push_pop() {
        let mut fm = FocusManager::new(F::Base);
        assert_eq!(fm.active(), F::Base);
        fm.push(F::A);
        assert_eq!(fm.active(), F::A);
        fm.push(F::B);
        assert_eq!(fm.active(), F::B);
        fm.pop();
        assert_eq!(fm.active(), F::A);
        fm.clear_to_base();
        assert_eq!(fm.active(), F::Base);
    }
}
