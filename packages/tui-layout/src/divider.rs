//! Local two-child divider geometry.
//!
//! A divider is intentionally not another recursive flex node.  It solves one
//! already-positioned pair of children and leaves theme-specific painting to
//! the product crate.

use ratatui::layout::Rect;

/// Direction in which the two children are laid out.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SplitAxis {
    /// First child on the left, second child on the right.
    Horizontal,
    /// First child on top, second child on the bottom.
    Vertical,
}

/// Size request for the first child.  The second child receives the remainder.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SplitSize {
    Fixed(u16),
    Percent(u16),
    /// A balanced split of the space left after the divider.
    Grow,
}

/// A local split specification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DividerSplit {
    pub axis: SplitAxis,
    pub first: SplitSize,
    pub divider: u16,
}

/// Result of solving a [`DividerSplit`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DividerPlan {
    pub first: Rect,
    pub divider: Option<Rect>,
    pub second: Rect,
}

impl DividerSplit {
    pub const fn new(axis: SplitAxis, first: SplitSize, divider: u16) -> Self {
        Self {
            axis,
            first,
            divider,
        }
    }

    pub fn solve(self, area: Rect) -> DividerPlan {
        solve(area, self)
    }
}

/// Solve one bounded two-child split.
pub fn solve(area: Rect, split: DividerSplit) -> DividerPlan {
    let total = match split.axis {
        SplitAxis::Horizontal => area.width,
        SplitAxis::Vertical => area.height,
    };
    let divider_size = split.divider.min(total);
    let available = total.saturating_sub(divider_size);
    let first_size = match split.first {
        SplitSize::Fixed(size) => size.min(available),
        SplitSize::Percent(percent) => {
            ((u32::from(available) * u32::from(percent.min(100))) / 100) as u16
        }
        SplitSize::Grow => available / 2,
    };
    let second_size = available.saturating_sub(first_size);
    let divider = (divider_size > 0).then(|| match split.axis {
        SplitAxis::Horizontal => Rect::new(
            area.x.saturating_add(first_size),
            area.y,
            divider_size,
            area.height,
        ),
        SplitAxis::Vertical => Rect::new(
            area.x,
            area.y.saturating_add(first_size),
            area.width,
            divider_size,
        ),
    });
    let (first, second) = match split.axis {
        SplitAxis::Horizontal => (
            Rect::new(area.x, area.y, first_size, area.height),
            Rect::new(
                area.x
                    .saturating_add(first_size)
                    .saturating_add(divider_size),
                area.y,
                second_size,
                area.height,
            ),
        ),
        SplitAxis::Vertical => (
            Rect::new(area.x, area.y, area.width, first_size),
            Rect::new(
                area.x,
                area.y
                    .saturating_add(first_size)
                    .saturating_add(divider_size),
                area.width,
                second_size,
            ),
        ),
    };
    DividerPlan {
        first,
        divider,
        second,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_split_clamps_bands() {
        let plan = DividerSplit::new(SplitAxis::Horizontal, SplitSize::Fixed(99), 2)
            .solve(Rect::new(3, 4, 5, 2));
        assert_eq!(plan.first, Rect::new(3, 4, 3, 2));
        assert_eq!(plan.divider, Some(Rect::new(6, 4, 2, 2)));
        assert_eq!(plan.second, Rect::new(8, 4, 0, 2));
    }

    #[test]
    fn vertical_percent_and_zero_divider() {
        let plan = DividerSplit::new(SplitAxis::Vertical, SplitSize::Percent(50), 0)
            .solve(Rect::new(1, 2, 8, 5));
        assert_eq!(plan.first, Rect::new(1, 2, 8, 2));
        assert_eq!(plan.divider, None);
        assert_eq!(plan.second, Rect::new(1, 4, 8, 3));
    }
}
