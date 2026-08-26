//! Product-neutral component interaction primitives.

use ratatui::layout::Rect;

/// A hit resolved by the shared hit map and scoped to its owning component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentHit<E> {
    pub element: Option<E>,
    pub rect: Rect,
    pub x: u16,
    pub y: u16,
}

impl<E> ComponentHit<E> {
    pub fn local_x(&self) -> u16 {
        self.x.saturating_sub(self.rect.x)
    }

    pub fn local_y(&self) -> u16 {
        self.y.saturating_sub(self.rect.y)
    }
}

/// Pointer gestures after terminal-adapter event normalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerGesture {
    Activate,
    ScrollUp,
    ScrollDown,
}

/// Per-frame interaction state scoped to one component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractionState<E> {
    pub hovered: Option<E>,
}

impl<E> Default for InteractionState<E> {
    fn default() -> Self {
        Self { hovered: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_coordinates_are_relative_to_hit_rect() {
        let hit = ComponentHit {
            element: Some(1),
            rect: Rect::new(10, 5, 20, 4),
            x: 14,
            y: 7,
        };
        assert_eq!(hit.local_x(), 4);
    }
}
