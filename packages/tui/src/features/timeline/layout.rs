use ratatui::{layout::Rect, text::Line};

use crate::{app::HitId, layout::DEFAULT_HORIZONTAL_INSET, theme::Theme};

use super::{Timeline, TimelineComponent};

pub(crate) struct TimelineRenderPlan {
    pub lines: Vec<Line<'static>>,
    pub content_area: Rect,
    pub top_offset: usize,
    pub tool_regions: Vec<(Rect, HitId)>,
}

impl Timeline {
    pub(crate) fn render_plan(
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
        // Hit targets are title-row only (not the full expanded body).
        // tool_lines always lays out: pad · title · … so title is start+1.
        let mut tool_title_rows: Vec<(usize, usize)> = Vec::new();
        let mut seen_ids = Vec::with_capacity(self.components.len());
        let mut cache = self.line_cache.borrow_mut();
        for (source_index, component) in self.components.iter().enumerate() {
            seen_ids.push(component.id().clone());
            let body = cache.lines_for(
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
            lines.extend(body);
            if matches!(component, TimelineComponent::Tool(_)) {
                // Title row is the second line of every tool card (after top pad).
                let title_row = start.saturating_add(super::render::TOOL_TITLE_ROW_OFFSET);
                if title_row < lines.len() {
                    tool_title_rows.push((source_index, title_row));
                }
            }
        }
        cache.retain_ids(&seen_ids);
        drop(cache);

        let has_pending = self.viewport.pending_new_items() > 0;
        let visible_height =
            usize::from(content_area.height.saturating_sub(u16::from(has_pending))).max(1);
        self.viewport.set_metrics(lines.len(), visible_height);
        let top_offset = self.viewport.top_offset();
        let visible_end = top_offset.saturating_add(visible_height);
        let content_bottom = content_area.y.saturating_add(content_area.height);
        let tool_regions = tool_title_rows
            .into_iter()
            .filter_map(|(source_index, title_row)| {
                if title_row < top_offset || title_row >= visible_end {
                    return None;
                }
                let y = content_area
                    .y
                    .saturating_add((title_row - top_offset).min(u16::MAX as usize) as u16);
                if y >= content_bottom {
                    return None;
                }
                Some((
                    Rect::new(content_area.x, y, content_area.width, 1),
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

    #[cfg(test)]
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
        // Keep a stable collapsed card height for layout geometry tests:
        // empty args + plain-text result → title + preview row (not title_meta).
        ToolEntry::new(
            id.into(),
            "tool".into(),
            ToolStatus::Completed,
            String::new(),
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

        // Collapsed cards are short (title [+ preview]); pin to latest and
        // assert hit geometry rather than fixed historical heights.
        let latest = timeline.pointer_regions(area, &theme);
        assert!(
            !latest.is_empty(),
            "expected at least the latest tool hit: {latest:?}"
        );
        assert_eq!(
            latest.last().map(|(_, id)| *id),
            Some(HitId::TimelineTool(1))
        );
        // Visible hits are ordered top→bottom and do not overlap.
        for window in latest.windows(2) {
            let (a, _) = window[0];
            let (b, _) = window[1];
            assert!(a.y <= b.y, "hits should be top-to-bottom: {latest:?}");
            assert!(
                a.y.saturating_add(a.height) <= b.y,
                "hits must not overlap: {latest:?}"
            );
        }

        timeline.scroll_up(3);
        let scrolled = timeline.pointer_regions(area, &theme);
        assert!(
            scrolled.iter().any(|(_, id)| *id == HitId::TimelineTool(0)),
            "scroll-up should reveal older tool: {scrolled:?}"
        );
        // Gap rows between cards are not hit targets.
        for window in scrolled.windows(2) {
            let (a, _) = window[0];
            let (b, _) = window[1];
            assert!(
                a.y.saturating_add(a.height) <= b.y,
                "hits must not overlap after scroll: {scrolled:?}"
            );
        }
    }

    #[test]
    fn tool_click_emits_block_specific_toggle_action() {
        let mut timeline = Timeline::new();
        timeline.push(TimelineEntry::Tool(tool("one", "first")));
        let actions = timeline.pointer_event(
            ComponentHit {
                element: Some(HitId::TimelineTool(0)),
                rect: Rect::new(2, 0, 37, 1),
                x: 3,
                y: 0,
            },
            PointerGesture::Activate,
        );
        assert!(matches!(
            actions.as_slice(),
            [Action::Timeline(TimelineAction::ToggleTool(0))]
        ));
    }

    #[test]
    fn tool_hit_region_is_title_row_only_even_when_expanded() {
        let theme = Theme::dark();
        let mut timeline = Timeline::new();
        let mut t = tool("one", "first");
        t.expanded = true;
        // Give the body some height so a full-block hit would be taller than 1.
        t.result = Some("line1\nline2\nline3\nline4\nline5".into());
        timeline.push(TimelineEntry::Tool(t));
        let area = Rect::new(0, 0, 40, 20);
        let regions = timeline.pointer_regions(area, &theme);
        assert_eq!(regions.len(), 1, "one tool hit: {regions:?}");
        let (rect, id) = regions[0];
        assert_eq!(id, HitId::TimelineTool(0));
        assert_eq!(rect.height, 1, "hit must be title-row only, got {rect:?}");
    }

    fn apply_text_delta(timeline: &mut Timeline, seq: u64, text: &str) {
        let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
            Some("s".into()),
            Some("root".into()),
            "msg",
            Some(seq),
            &piko_protocol::agent_runtime::RealtimeDelta::Text {
                content_index: 0,
                delta: text.into(),
            },
        )
        .pop()
        .unwrap();
        timeline.apply_stream_item(&patch);
    }

    #[test]
    fn projection_batch_rebuilds_components_once_at_end() {
        let mut timeline = Timeline::new();
        timeline.begin_projection_batch();
        apply_text_delta(&mut timeline, 1, "hello");
        assert!(
            timeline.components.is_empty(),
            "batch must defer presentation rebuild"
        );
        apply_text_delta(&mut timeline, 2, " world");
        assert!(timeline.components.is_empty());
        timeline.end_projection_batch();
        assert_eq!(
            timeline.assistant_text("msg").as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn render_plan_is_stable_when_line_cache_hits() {
        let theme = Theme::dark();
        let mut timeline = Timeline::new();
        apply_text_delta(&mut timeline, 1, "cached body");
        let area = Rect::new(0, 0, 40, 12);
        let first = timeline.render_plan(area, &theme, None);
        let second = timeline.render_plan(area, &theme, None);
        assert_eq!(first.lines.len(), second.lines.len());
        assert_eq!(first.top_offset, second.top_offset);
        assert_eq!(
            first
                .lines
                .iter()
                .map(|line| line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>())
                .collect::<Vec<_>>(),
            second
                .lines
                .iter()
                .map(|line| line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>())
                .collect::<Vec<_>>(),
        );
    }
}
