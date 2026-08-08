//! Rect helpers (no product meaning).

use ratatui::layout::Rect;

/// Default left/right inset often used for edge-flush content leaves.
pub const DEFAULT_HORIZONTAL_INSET: u16 = 1;

/// Shrink `area` by `inset` cells on the left and right only.
pub fn inset_horizontal(area: Rect, inset: u16) -> Rect {
    if area.width == 0 {
        return area;
    }
    let horizontal = inset.min(area.width.saturating_sub(1) / 2);
    Rect {
        x: area.x.saturating_add(horizontal),
        y: area.y,
        width: area.width.saturating_sub(horizontal.saturating_mul(2)),
        height: area.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inset_gutter() {
        assert_eq!(
            inset_horizontal(Rect::new(0, 0, 80, 24), 1),
            Rect::new(1, 0, 78, 24)
        );
    }
}
