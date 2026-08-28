use std::time::Instant;

use piko_tui_layout::{ContentHitPlan, ViewportPlan, row_owners};
use ratatui::{layout::Rect, text::Line};

use crate::{app::HitId, layout::DEFAULT_HORIZONTAL_INSET, theme::Theme};

use super::{Timeline, TimelineComponent};

/// Stable owner of one content row. Hits are resolved from this in content
/// space at event time; screen coordinates are never baked into the plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RowOwner {
    /// Tool title row, keyed by the interned tool identity.
    Tool(u64),
    /// One-row thought summary, keyed by its interned semantic identity.
    Thought(u64),
}

pub(crate) struct TimelineRenderPlan {
    pub lines: Vec<Line<'static>>,
    pub content_area: Rect,
    /// Full stream region (including the scrollbar gutter / banner row).
    pub stream_rect: Rect,
    /// Paint-time snapshot of the viewport top offset. Hit-testing reads the
    /// viewport live instead, so scroll can never make this plan stale.
    pub top_offset: usize,
    /// Generic content-space resolver shared with other scrollable surfaces.
    pub content_hits: ContentHitPlan<RowOwner>,
    /// Shared viewport geometry, including the always-reserved scrollbar
    /// gutter and the current scrollbar metrics.
    pub viewport: ViewportPlan,
    /// `Timeline::layout_epoch` at plan time; mismatch triggers a recompute
    /// before an event is routed against this plan.
    pub epoch: u64,
    pub spinner_frame: usize,
    pub rendered_at: Instant,
}

impl TimelineRenderPlan {
    /// Resolve a screen coordinate inside the stream against the **current**
    /// viewport offset (read at event time). Returns the tool hit and its
    /// one-row title rect, or `None` for the Stream default (message/gap rows,
    /// banner row, scrollbar gutter, out-of-range content).
    pub(crate) fn resolve(&self, x: u16, y: u16, top_offset: usize) -> Option<(HitId, Rect)> {
        let resolved = self.content_hits.resolve(top_offset, x, y)?;
        let hit = match resolved.owner {
            RowOwner::Tool(id) => HitId::TimelineTool(id),
            RowOwner::Thought(id) => HitId::TimelineThought(id),
        };
        Some((hit, resolved.rect))
    }
}

impl Timeline {
    #[cfg(test)]
    pub(crate) fn render_plan(
        &self,
        area: Rect,
        theme: &Theme,
        hovered_tool: Option<u64>,
    ) -> TimelineRenderPlan {
        self.render_plan_at(
            area,
            theme,
            hovered_tool.map(HitId::TimelineTool),
            0,
            Instant::now(),
        )
    }

    pub(crate) fn render_plan_at(
        &self,
        area: Rect,
        theme: &Theme,
        hovered: Option<HitId>,
        spinner_frame: usize,
        now: Instant,
    ) -> TimelineRenderPlan {
        let content_band = Rect {
            x: area.x,
            y: area.y,
            width: area.width.saturating_sub(1),
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
        // Content-space tool title rows (pad · title · … so title is start+1).
        // Hit identity is the interned tool id, not the component slot.
        let mut tool_title_rows: Vec<(usize, u64)> = Vec::new();
        let mut thought_rows: Vec<(usize, u64)> = Vec::new();
        let mut seen_ids = Vec::with_capacity(self.components.len());
        let mut cache = self.line_cache.borrow_mut();
        for component in self.components.iter() {
            seen_ids.push(component.id().clone());
            let component_hovered = match component {
                TimelineComponent::Tool(tool) => {
                    hovered == self.hit_ids.get(&tool.id).copied().map(HitId::TimelineTool)
                }
                TimelineComponent::Thought(thought) => {
                    self.thought_hit_ids
                        .get(&thought.key)
                        .copied()
                        .map(HitId::TimelineThought)
                        == hovered
                }
                _ => false,
            };
            let body = if matches!(component, TimelineComponent::Thought(_)) {
                super::render::component_lines_at(
                    component,
                    self.thinking_visible,
                    component_hovered,
                    theme,
                    content_area.width,
                    spinner_frame,
                    now,
                )
            } else {
                cache.lines_for(
                    component,
                    self.thinking_visible,
                    component_hovered,
                    theme,
                    content_area.width,
                )
            };
            if body.is_empty() {
                continue;
            }
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            let start = lines.len();
            lines.extend(body);
            if let TimelineComponent::Tool(tool) = component
                && let Some(hit_id) = self.hit_ids.get(&tool.id).copied()
            {
                // Title row is the second line of every tool card (after top pad).
                let title_row = start.saturating_add(super::render::TOOL_TITLE_ROW_OFFSET);
                tool_title_rows.push((title_row, hit_id));
            }
            if let TimelineComponent::Thought(thought) = component
                && let Some(hit_id) = self.thought_hit_ids.get(&thought.key).copied()
            {
                thought_rows.push((start, hit_id));
            }
        }
        cache.retain_ids(&seen_ids);
        drop(cache);
        self.selection
            .borrow_mut()
            .update_snapshot(&lines, self.layout_epoch);

        let mut owners = vec![None; lines.len()];
        for (title_row, hit_id) in tool_title_rows {
            if title_row < owners.len() {
                owners[title_row] = Some(RowOwner::Tool(hit_id));
            }
        }
        for (row, hit_id) in thought_rows {
            if row < owners.len() {
                owners[row] = Some(RowOwner::Thought(hit_id));
            }
        }

        let has_pending = self.viewport.pending_new_items() > 0;
        let visible_height =
            usize::from(content_area.height.saturating_sub(u16::from(has_pending))).max(1);
        self.viewport.set_metrics(lines.len(), visible_height);
        let top_offset = self.viewport.top_offset();
        let viewport_outer = Rect::new(
            content_area.x,
            content_area.y,
            content_area.width.saturating_add(u16::from(area.width > 0)),
            content_area.height,
        );
        let viewport = self.viewport.prepare(viewport_outer);

        let content_hits = ContentHitPlan::new(
            content_area,
            Rect::new(
                content_area.x,
                content_area.y,
                content_area.width,
                visible_height.min(usize::from(content_area.height)) as u16,
            ),
            row_owners(owners.iter().copied()),
            self.layout_epoch,
        );

        TimelineRenderPlan {
            lines,
            content_area,
            stream_rect: area,
            top_offset,
            content_hits,
            viewport,
            epoch: self.layout_epoch,
            spinner_frame,
            rendered_at: now,
        }
    }

    #[cfg(test)]
    pub(crate) fn tool_hits(&self, area: Rect, theme: &Theme) -> Vec<(u64, Rect)> {
        let plan = self.render_plan(area, theme, None);
        let top = self.viewport.top_offset();
        (plan.content_area.y..plan.content_area.bottom())
            .filter_map(|y| {
                plan.resolve(plan.content_area.x, y, top)
                    .and_then(|(id, rect)| match id {
                        HitId::TimelineTool(id) => Some((id, rect)),
                        _ => None,
                    })
            })
            .collect()
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

        timeline.toggle_tool(timeline.hit_ids["one"]);
        assert!(expanded(&timeline, 0));
        assert!(!expanded(&timeline, 1));

        assert!(timeline.upsert_tool(tool("one", "updated")));
        assert!(expanded(&timeline, 0));
        assert!(timeline.tool_calls[0].expanded);

        // The interned identity survives a clear → rebuild (id is never
        // reused, and the rebuilt tool gets a fresh non-expanded state).
        timeline.clear();
        timeline.push(TimelineEntry::Tool(tool("one", "rebuilt")));
        assert!(!expanded(&timeline, 0));
    }

    #[test]
    fn tool_hits_are_title_rows_and_follow_scroll() {
        let theme = Theme::dark();
        let mut timeline = Timeline::new();
        timeline.push(TimelineEntry::Tool(tool("one", "first")));
        timeline.push(TimelineEntry::Tool(tool("two", "second")));
        let area = Rect::new(0, 0, 40, 4);

        // Collapsed cards are short; pin to latest and assert hit geometry
        // rather than fixed historical heights.
        let latest = timeline.tool_hits(area, &theme);
        assert!(
            !latest.is_empty(),
            "expected at least the latest tool hit: {latest:?}"
        );
        // Latest tool (interned id 2) is visible at the bottom.
        assert_eq!(latest.last().map(|(id, _)| *id), Some(2));
        // Tool hits are title-row only.
        for (_, rect) in &latest {
            assert_eq!(rect.height, 1, "hit must be title-row only: {rect:?}");
        }
        // Visible hits are ordered top→bottom and do not overlap.
        for window in latest.windows(2) {
            let (_, a) = window[0];
            let (_, b) = window[1];
            assert!(a.y <= b.y, "hits should be top-to-bottom: {latest:?}");
        }

        timeline.scroll_up(3);
        let scrolled = timeline.tool_hits(area, &theme);
        assert!(
            scrolled.iter().any(|(id, _)| *id == 1),
            "scroll-up should reveal older tool: {scrolled:?}"
        );
        // Gap rows between cards are not hit targets.
        let plan = timeline.render_plan(area, &theme, None);
        let top = timeline.viewport.top_offset();
        for window in scrolled.windows(2) {
            let (_, a) = window[0];
            let (_, b) = window[1];
            if a.y + 1 < b.y {
                assert_eq!(
                    plan.resolve(plan.content_area.x, a.y + 1, top),
                    None,
                    "gap row must not be a tool hit"
                );
            }
        }
    }

    #[test]
    fn tool_click_emits_block_specific_toggle_action() {
        let mut timeline = Timeline::new();
        timeline.push(TimelineEntry::Tool(tool("one", "first")));
        let hit_id = timeline
            .hit_ids
            .get("one")
            .copied()
            .expect("interned hit id");
        let actions = timeline.pointer_event(
            ComponentHit {
                element: Some(HitId::TimelineTool(hit_id)),
                rect: Rect::new(2, 0, 37, 1),
                x: 3,
                y: 0,
            },
            PointerGesture::Activate,
        );
        assert!(matches!(
            actions.as_slice(),
            [Action::Timeline(TimelineAction::ToggleTool(id))] if *id == hit_id
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
        let hits = timeline.tool_hits(area, &theme);
        assert_eq!(hits.len(), 1, "one tool hit: {hits:?}");
        let (id, rect) = hits[0];
        assert_eq!(id, timeline.hit_ids["one"]);
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

    #[test]
    fn scroll_never_bumps_layout_epoch() {
        let theme = Theme::dark();
        let mut timeline = Timeline::new();
        timeline.push(TimelineEntry::Tool(tool("one", "first")));
        timeline.push(TimelineEntry::Tool(tool("two", "second")));
        let area = Rect::new(0, 0, 40, 4);

        let plan = timeline.render_plan(area, &theme, None);
        assert_eq!(plan.epoch, timeline.layout_epoch());

        timeline.scroll_up(3);
        timeline.scroll_down(1);
        timeline.jump_latest();
        assert_eq!(
            plan.epoch,
            timeline.layout_epoch(),
            "pure scroll must not invalidate the retained plan"
        );

        timeline.push(TimelineEntry::Tool(tool("three", "third")));
        assert_ne!(
            plan.epoch,
            timeline.layout_epoch(),
            "content mutation must bump the layout epoch"
        );
    }

    #[test]
    fn hit_identity_survives_projection_rebuild() {
        let mut timeline = Timeline::new();
        timeline.project_tool_started(
            "call-1".into(),
            "bash".into(),
            serde_json::json!({ "cmd": "true" }),
            None,
        );
        let hit_id = timeline.hit_ids["call-1"];
        assert_eq!(timeline.tool_expanded("call-1"), Some(false));

        // A content change forces a full projection rebuild; the interned hit
        // identity must be preserved so existing hits never retarget.
        apply_text_delta(&mut timeline, 1, "hello");
        assert_eq!(timeline.hit_ids["call-1"], hit_id);

        timeline.toggle_tool(hit_id);
        assert_eq!(timeline.tool_expanded("call-1"), Some(true));
        assert!(timeline.tool_calls.iter().any(|tool| tool.expanded));
    }
}
