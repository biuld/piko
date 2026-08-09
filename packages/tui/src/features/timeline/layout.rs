use ratatui::{layout::Rect, text::Line};

use crate::{app::HitId, layout::DEFAULT_HORIZONTAL_INSET, theme::Theme};

use super::{Timeline, TimelineComponent, render::component_lines};

pub(super) struct TimelineRenderPlan {
    pub lines: Vec<Line<'static>>,
    pub content_area: Rect,
    pub top_offset: usize,
    pub tool_regions: Vec<(Rect, HitId)>,
}

impl Timeline {
    pub(super) fn render_plan(
        &self,
        area: Rect,
        theme: &Theme,
        hovered_tool: Option<usize>,
    ) -> TimelineRenderPlan {
        let content_band = Rect {
            x: area.x,
            y: area.y,
            width: area.width.saturating_sub(1).max(1),
            height: area.height,
        };
        let left = DEFAULT_HORIZONTAL_INSET.min(content_band.width.saturating_sub(1));
        let content_area = Rect {
            x: content_band.x.saturating_add(left),
            y: content_band.y,
            width: content_band.width.saturating_sub(left),
            height: content_band.height,
        };

        let mut lines = Vec::new();
        let mut tool_ranges = Vec::new();
        for (source_index, component) in self.components.iter().enumerate() {
            let body = component_lines(
                component,
                self.thinking_visible,
                hovered_tool == Some(source_index),
                theme,
                content_area.width,
            );
            if body.is_empty() {
                continue;
            }
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            let start = lines.len();
            let height = body.len();
            lines.extend(body);
            if matches!(component, TimelineComponent::Tool(_)) {
                tool_ranges.push((source_index, start, height));
            }
        }

        let has_pending = self.viewport.pending_new_items() > 0;
        let visible_height =
            usize::from(content_area.height.saturating_sub(u16::from(has_pending))).max(1);
        self.viewport.set_metrics(lines.len(), visible_height);
        let top_offset = self.viewport.top_offset();
        let visible_end = top_offset.saturating_add(visible_height);
        let content_bottom = content_area.y.saturating_add(content_area.height);
        let tool_regions = tool_ranges
            .into_iter()
            .filter_map(|(source_index, start, height)| {
                let end = start.saturating_add(height);
                let clipped_start = start.max(top_offset);
                let clipped_end = end.min(visible_end);
                if clipped_start >= clipped_end {
                    return None;
                }
                let y = content_area
                    .y
                    .saturating_add((clipped_start - top_offset).min(u16::MAX as usize) as u16);
                let height = (clipped_end - clipped_start).min(u16::MAX as usize) as u16;
                let height = height.min(content_bottom.saturating_sub(y));
                (height > 0).then_some((
                    Rect::new(content_area.x, y, content_area.width, height),
                    HitId::TimelineTool(source_index),
                ))
            })
            .collect();

        TimelineRenderPlan {
            lines,
            content_area,
            top_offset,
            tool_regions,
        }
    }

    pub(crate) fn pointer_regions(&self, area: Rect, theme: &Theme) -> Vec<(Rect, HitId)> {
        self.render_plan(area, theme, None).tool_regions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{
            ToolStatus,
            command::{Action, TimelineAction},
        },
        features::timeline::{TimelineEntry, ToolEntry},
        ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
    };

    fn tool(id: &str, result: &str) -> ToolEntry {
        ToolEntry::new(
            id.into(),
            "bash".into(),
            ToolStatus::Completed,
            r#"{"cmd":"true"}"#.into(),
            Some(result.into()),
            None,
        )
    }

    fn expanded(timeline: &Timeline, index: usize) -> bool {
        matches!(
            timeline.components.get(index),
            Some(TimelineComponent::Tool(tool)) if tool.expanded
        )
    }

    #[test]
    fn tool_expansion_is_independent_and_survives_upsert() {
        let mut timeline = Timeline::new();
        timeline.push(TimelineEntry::Tool(tool("one", "first")));
        timeline.push(TimelineEntry::Tool(tool("two", "second")));

        timeline.toggle_tool(0);
        assert!(expanded(&timeline, 0));
        assert!(!expanded(&timeline, 1));

        assert!(timeline.upsert_tool(tool("one", "updated")));
        assert!(expanded(&timeline, 0));
        assert!(timeline.tool_calls[0].expanded);

        timeline.clear();
        timeline.push(TimelineEntry::Tool(tool("one", "rebuilt")));
        assert!(!expanded(&timeline, 0));
    }

    #[test]
    fn tool_regions_are_clipped_with_blocks_and_preserve_gap_rows() {
        let theme = Theme::dark();
        let mut timeline = Timeline::new();
        timeline.push(TimelineEntry::Tool(tool("one", "first")));
        timeline.push(TimelineEntry::Tool(tool("two", "second")));
        let area = Rect::new(0, 0, 40, 4);

        let latest = timeline.pointer_regions(area, &theme);
        assert_eq!(
            latest,
            vec![(Rect::new(1, 0, 38, 4), HitId::TimelineTool(1))]
        );

        timeline.scroll_up(3);
        let scrolled = timeline.pointer_regions(area, &theme);
        assert_eq!(
            scrolled,
            vec![
                (Rect::new(1, 0, 38, 2), HitId::TimelineTool(0)),
                (Rect::new(1, 3, 38, 1), HitId::TimelineTool(1)),
            ]
        );
        assert!(
            scrolled
                .iter()
                .all(|(rect, _)| { !(rect.y <= 2 && 2 < rect.y.saturating_add(rect.height)) })
        );
    }

    #[test]
    fn tool_click_emits_block_specific_toggle_action() {
        let mut timeline = Timeline::new();
        timeline.push(TimelineEntry::Tool(tool("one", "first")));
        let actions = timeline.pointer_event(
            ComponentHit {
                element: Some(HitId::TimelineTool(0)),
                rect: Rect::new(2, 0, 37, 4),
                x: 3,
                y: 1,
            },
            PointerGesture::Activate,
        );
        assert!(matches!(
            actions.as_slice(),
            [Action::Timeline(TimelineAction::ToggleTool(0))]
        ));
    }
}
