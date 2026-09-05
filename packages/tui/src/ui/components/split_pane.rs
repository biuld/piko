//! Responsive two-pane composition inside existing surface chrome.

use piko_tui_layout::{DividerSplit, SplitAxis, SplitSize};
use ratatui::{Frame, layout::Rect};

use super::{divider::paint_divider_with_axis, pane::PanePadding};
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneSide {
    #[default]
    First,
    Second,
}

#[derive(Clone, Copy, Debug)]
pub struct SplitPaneSpec {
    pub first: SplitSize,
    pub minimum: [u16; 2],
    pub padding: PanePadding,
    pub separator: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneRegion {
    pub outer: Rect,
    pub content: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitPanePlan {
    pub first: Option<PaneRegion>,
    pub second: Option<PaneRegion>,
    pub divider: Option<Rect>,
}

impl SplitPaneSpec {
    pub fn prepare(self, area: Rect, compact: PaneSide) -> SplitPanePlan {
        let inset = u32::from(self.padding.horizontal) * 2;
        let required = u32::from(self.minimum[0])
            + u32::from(self.minimum[1])
            + inset * 2
            + u32::from(self.separator);
        if u32::from(area.width) < required {
            let region = Some(self.region(area));
            return SplitPanePlan {
                first: (compact == PaneSide::First).then_some(region).flatten(),
                second: (compact == PaneSide::Second).then_some(region).flatten(),
                divider: None,
            };
        }
        let available = area.width.saturating_sub(self.separator);
        let requested = DividerSplit::new(SplitAxis::Horizontal, self.first, self.separator)
            .solve(area)
            .first
            .width;
        let minimum = self.minimum[0].saturating_add(inset as u16);
        let maximum = available.saturating_sub(self.minimum[1].saturating_add(inset as u16));
        let plan = DividerSplit::new(
            SplitAxis::Horizontal,
            SplitSize::Fixed(requested.clamp(minimum, maximum)),
            self.separator,
        )
        .solve(area);
        SplitPanePlan {
            first: Some(self.region(plan.first)),
            second: Some(self.region(plan.second)),
            divider: plan.divider,
        }
    }

    fn region(self, outer: Rect) -> PaneRegion {
        let horizontal = self.padding.horizontal.min(outer.width / 2);
        let vertical = self.padding.vertical.min(outer.height / 2);
        PaneRegion {
            outer,
            content: Rect::new(
                outer.x.saturating_add(horizontal),
                outer.y.saturating_add(vertical),
                outer.width.saturating_sub(horizontal * 2),
                outer.height.saturating_sub(vertical * 2),
            ),
        }
    }
}

impl SplitPanePlan {
    pub fn is_wide(self) -> bool {
        self.first.is_some() && self.second.is_some()
    }

    pub fn pane_at(self, x: u16, y: u16) -> Option<PaneSide> {
        [
            (PaneSide::First, self.first),
            (PaneSide::Second, self.second),
        ]
        .into_iter()
        .find_map(|(side, region)| {
            region
                .filter(|region| region.content.contains((x, y).into()))
                .map(|_| side)
        })
    }

    pub fn paint(self, frame: &mut Frame<'_>, theme: &Theme, active: PaneSide) {
        if let Some(divider) = self.divider {
            paint_divider_with_axis(frame, divider, SplitAxis::Horizontal, theme);
        }
        let region = match active {
            PaneSide::First => self.first,
            PaneSide::Second => self.second,
        };
        if let Some(region) = region.filter(|region| {
            region.outer.width > 0 && region.outer.height > 0 && region.content.x > region.outer.x
        }) {
            frame.render_widget(
                ratatui::widgets::Paragraph::new("▸")
                    .style(ratatui::style::Style::default().fg(theme.accent)),
                Rect::new(region.outer.x, region.outer.y, 1, 1),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SplitPaneSpec {
        SplitPaneSpec {
            first: SplitSize::Percent(46),
            minimum: [30, 40],
            padding: PanePadding::new(1, 0),
            separator: 1,
        }
    }

    #[test]
    fn minimums_control_fallback_and_hit_regions() {
        let wide = spec().prepare(Rect::new(5, 3, 75, 12), PaneSide::Second);
        assert!(wide.is_wide());
        assert_eq!(wide.first.unwrap().content.width, 30);
        assert_eq!(wide.second.unwrap().content.width, 40);
        assert_eq!(wide.pane_at(5, 3), None);
        assert_eq!(wide.pane_at(6, 3), Some(PaneSide::First));
        assert_eq!(wide.pane_at(wide.divider.unwrap().x, 3), None);
        let narrow = spec().prepare(Rect::new(5, 3, 74, 12), PaneSide::Second);
        assert!(narrow.first.is_none());
        assert_eq!(narrow.second.unwrap().content, Rect::new(6, 3, 72, 12));
    }

    #[test]
    fn tiny_areas_are_bounded_and_have_no_hits() {
        for width in 0..3 {
            let area = Rect::new(8, 4, width, 0);
            let plan = spec().prepare(area, PaneSide::First);
            assert_eq!(plan.pane_at(8, 4), None);
            assert!(plan.first.unwrap().content.right() <= area.right());
        }
    }
}
