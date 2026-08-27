//! Small, saturating geometry helpers for local terminal layouts.
//!
//! The shell/flex solver owns region placement.  This module is deliberately
//! smaller: it only insets, reserves, clips, or aligns an already solved
//! [`Rect`].  All helpers are total for zero-sized and undersized areas.

use ratatui::layout::Rect;

/// Four-sided cell budget removed from an area.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Padding {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Padding {
    pub const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub const fn new(top: u16, right: u16, bottom: u16, left: u16) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn uniform(cells: u16) -> Self {
        Self::new(cells, cells, cells, cells)
    }

    pub const fn horizontal(cells: u16) -> Self {
        Self::new(0, cells, 0, cells)
    }

    pub const fn vertical(cells: u16) -> Self {
        Self::new(cells, 0, cells, 0)
    }

    /// Apply this budget without underflowing or escaping `area`.
    ///
    /// When opposite budgets do not fit, the leading side is consumed first
    /// and the trailing side receives the remaining cells.  This makes the
    /// result deterministic while ensuring the returned rectangle is bounded.
    pub fn apply(self, area: Rect) -> Rect {
        let left = self.left.min(area.width);
        let right = self.right.min(area.width.saturating_sub(left));
        let top = self.top.min(area.height);
        let bottom = self.bottom.min(area.height.saturating_sub(top));

        Rect::new(
            area.x.saturating_add(left),
            area.y.saturating_add(top),
            area.width.saturating_sub(left).saturating_sub(right),
            area.height.saturating_sub(top).saturating_sub(bottom),
        )
    }
}

/// Which edge owns a reserved band.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum GutterSide {
    Left,
    Right,
    Top,
    Bottom,
}

/// A named, non-interactive band reserved beside content.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Gutter {
    pub side: GutterSide,
    pub size: u16,
}

/// A passive side reservation. It has the same bounded geometry as a
/// [`Gutter`], but names intentional empty space rather than a widget
/// affordance such as a scrollbar.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Spacer {
    pub side: GutterSide,
    pub size: u16,
}

impl Spacer {
    pub const fn new(side: GutterSide, size: u16) -> Self {
        Self { side, size }
    }

    /// Return `(content, spacer)` bounded by `area`.
    pub fn split(self, area: Rect) -> (Rect, Rect) {
        split_gutter(area, self.side, self.size)
    }
}

impl Gutter {
    pub const fn new(side: GutterSide, size: u16) -> Self {
        Self { side, size }
    }

    /// Return `(content, gutter)` with no overlap and both areas bounded by
    /// `area`.  A zero-sized gutter is represented by a zero-sized rectangle.
    pub fn split(self, area: Rect) -> (Rect, Rect) {
        split_gutter(area, self.side, self.size)
    }
}

/// Reserve a side band and return `(content, gutter)`.
pub fn split_gutter(area: Rect, side: GutterSide, requested: u16) -> (Rect, Rect) {
    match side {
        GutterSide::Left => {
            let size = requested.min(area.width);
            let gutter = Rect::new(area.x, area.y, size, area.height);
            let content = Rect::new(
                area.x.saturating_add(size),
                area.y,
                area.width.saturating_sub(size),
                area.height,
            );
            (content, gutter)
        }
        GutterSide::Right => {
            let size = requested.min(area.width);
            let content = Rect::new(area.x, area.y, area.width.saturating_sub(size), area.height);
            let gutter = Rect::new(
                area.x.saturating_add(area.width.saturating_sub(size)),
                area.y,
                size,
                area.height,
            );
            (content, gutter)
        }
        GutterSide::Top => {
            let size = requested.min(area.height);
            let gutter = Rect::new(area.x, area.y, area.width, size);
            let content = Rect::new(
                area.x,
                area.y.saturating_add(size),
                area.width,
                area.height.saturating_sub(size),
            );
            (content, gutter)
        }
        GutterSide::Bottom => {
            let size = requested.min(area.height);
            let content = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(size));
            let gutter = Rect::new(
                area.x,
                area.y.saturating_add(area.height.saturating_sub(size)),
                area.width,
                size,
            );
            (content, gutter)
        }
    }
}

/// Intersect two rectangles.  `None` means that no cell belongs to both.
pub fn intersection(first: Rect, second: Rect) -> Option<Rect> {
    let left = u32::from(first.x).max(u32::from(second.x));
    let top = u32::from(first.y).max(u32::from(second.y));
    let right = rect_right(first).min(rect_right(second));
    let bottom = rect_bottom(first).min(rect_bottom(second));
    (left < right && top < bottom).then(|| {
        Rect::new(
            left.min(u32::from(u16::MAX)) as u16,
            top.min(u32::from(u16::MAX)) as u16,
            (right - left).min(u32::from(u16::MAX)) as u16,
            (bottom - top).min(u32::from(u16::MAX)) as u16,
        )
    })
}

/// Clip `child` to `parent`, returning an empty rectangle when there is no
/// overlap.  Use [`intersection`] when absence needs to be distinguished.
pub fn clip(parent: Rect, child: Rect) -> Rect {
    intersection(parent, child).unwrap_or_else(|| Rect::new(parent.x, parent.y, 0, 0))
}

/// Alignment of a measured child inside an available area.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

/// Place a measured rectangle inside `area`, clamping its size first.
pub fn align(area: Rect, width: u16, height: u16, horizontal: Align, vertical: Align) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x_offset = match horizontal {
        Align::Start => 0,
        Align::Center => area.width.saturating_sub(width) / 2,
        Align::End => area.width.saturating_sub(width),
    };
    let y_offset = match vertical {
        Align::Start => 0,
        Align::Center => area.height.saturating_sub(height) / 2,
        Align::End => area.height.saturating_sub(height),
    };
    Rect::new(
        area.x.saturating_add(x_offset),
        area.y.saturating_add(y_offset),
        width,
        height,
    )
}

fn rect_right(rect: Rect) -> u32 {
    u32::from(rect.x).saturating_add(u32::from(rect.width))
}

fn rect_bottom(rect: Rect) -> u32 {
    u32::from(rect.y).saturating_add(u32::from(rect.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asymmetric_padding_saturates() {
        let area = Rect::new(4, 5, 3, 2);
        assert_eq!(Padding::new(9, 9, 9, 9).apply(area), Rect::new(7, 7, 0, 0));
    }

    #[test]
    fn gutter_is_reserved_even_when_empty() {
        let area = Rect::new(0, 0, 4, 2);
        let (content, gutter) = Gutter::new(GutterSide::Right, 1).split(area);
        assert_eq!(content, Rect::new(0, 0, 3, 2));
        assert_eq!(gutter, Rect::new(3, 0, 1, 2));

        let (content, spacer) = Spacer::new(GutterSide::Left, 2).split(area);
        assert_eq!(content, Rect::new(2, 0, 2, 2));
        assert_eq!(spacer, Rect::new(0, 0, 2, 2));
    }

    #[test]
    fn clip_and_align_are_bounded() {
        let area = Rect::new(2, 3, 5, 4);
        assert_eq!(clip(area, Rect::new(0, 0, 4, 5)), Rect::new(2, 3, 2, 2));
        assert_eq!(align(area, 99, 99, Align::End, Align::Center), area);
    }
}
