use ratatui::layout::Rect;

use super::{
    ComponentId, ThoughtComponent, ThoughtKey, ThoughtPhase, Timeline, TimelineComponent,
    TimelineEntry, ToolEntry,
};
use crate::{
    app::{HitId, ToolStatus, command::Action},
    theme::Theme,
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
};
use std::time::Instant;

fn tool(id: &str, result: &str) -> ToolEntry {
    ToolEntry::new(
        id.into(),
        "tool".into(),
        ToolStatus::Completed,
        String::new(),
        Some(result.into()),
        None,
    )
}

#[test]
fn resolve_tracks_live_scroll_offset_on_the_same_plan() {
    let theme = Theme::dark();
    let mut timeline = Timeline::new();
    for i in 0..8 {
        timeline.push(TimelineEntry::Tool(tool(&format!("t{i}"), "done")));
    }
    let area = Rect::new(0, 0, 40, 6);

    // Pin to latest and find the bottom-most visible tool title that has room
    // to shift down by one wheel step.
    let plan = timeline.render_plan(area, &theme, None);
    let hits = timeline.tool_hits(area, &theme);
    let (id, rect) = hits
        .iter()
        .rev()
        .find(|(_, r)| r.y + 3 < plan.content_area.bottom())
        .copied()
        .expect("visible tool with room to scroll");

    timeline.scroll_up(3);
    let new_top = timeline.viewport.top_offset();
    assert_eq!(
        plan.resolve(rect.x, rect.y + 3, new_top),
        Some((
            HitId::TimelineTool(id),
            Rect::new(rect.x, rect.y + 3, rect.width, 1)
        )),
        "hit must follow the tool through a live offset change"
    );
    assert_ne!(
        plan.resolve(rect.x, rect.y, new_top),
        Some((HitId::TimelineTool(id), rect))
    );
}

#[test]
fn non_tool_and_banner_rows_resolve_to_stream_default() {
    let theme = Theme::dark();
    let mut timeline = Timeline::new();
    timeline.push(TimelineEntry::Tool(tool("one", "first")));
    timeline.push(TimelineEntry::Tool(tool("two", "second")));
    let area = Rect::new(0, 0, 40, 4);

    let plan = timeline.render_plan(area, &theme, None);
    let top = timeline.viewport.top_offset();
    let hits = timeline.tool_hits(area, &theme);
    for window in hits.windows(2) {
        let (_, a) = window[0];
        let (_, b) = window[1];
        if a.y + 1 < b.y {
            assert_eq!(plan.resolve(plan.content_area.x, a.y + 1, top), None);
        }
    }
    // The scrollbar gutter column is outside the content band.
    assert_eq!(
        plan.resolve(plan.content_area.right(), plan.content_area.y, top),
        None
    );

    timeline.scroll_up(3);
    timeline.viewport.mark_appended();
    let pending = timeline.render_plan(area, &theme, None);
    let banner_y = pending.content_area.bottom() - 1;
    assert_eq!(
        pending.resolve(
            pending.content_area.x,
            banner_y,
            timeline.viewport.top_offset()
        ),
        None,
        "banner row must never resolve to a tool"
    );
}

#[test]
fn thought_hit_is_stable_and_resolves_the_whole_summary_row() {
    let theme = Theme::dark();
    let mut timeline = Timeline::new();
    let key = ThoughtKey {
        message_id: "message-1".into(),
        segment_index: 0,
    };
    timeline.push_component(TimelineComponent::Thought(ThoughtComponent {
        id: ComponentId::Thought(key.clone()),
        key: key.clone(),
        text: "long thought".into(),
        phase: ThoughtPhase::Completed {
            duration_ms: Some(1200),
        },
    }));
    let hit_id = timeline.thought_hit_id(&key).expect("thought hit id");
    let area = Rect::new(0, 0, 40, 8);
    let plan = timeline.render_plan_at(area, &theme, None, 0, Instant::now());
    let (resolved, rect) = plan
        .resolve(
            plan.content_area.x,
            plan.content_area.y,
            timeline.viewport.top_offset(),
        )
        .expect("thought row should resolve");
    assert_eq!(resolved, HitId::TimelineThought(hit_id));
    assert_eq!(rect.height, 1);

    let actions = timeline.pointer_event(
        ComponentHit {
            element: Some(HitId::TimelineThought(hit_id)),
            rect,
            x: rect.x,
            y: rect.y,
        },
        PointerGesture::Activate,
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Timeline(crate::app::command::TimelineAction::OpenThought(id))]
            if *id == hit_id
    ));

    timeline.clear();
    let next_key = ThoughtKey {
        message_id: "message-2".into(),
        segment_index: 0,
    };
    timeline.push_component(TimelineComponent::Thought(ThoughtComponent {
        id: ComponentId::Thought(next_key.clone()),
        key: next_key.clone(),
        text: "new thought".into(),
        phase: ThoughtPhase::Completed { duration_ms: None },
    }));
    let next_hit = timeline
        .thought_hit_id(&next_key)
        .expect("new thought hit id");
    assert_ne!(next_hit, hit_id, "clearing must not reuse thought hit ids");
    assert_eq!(timeline.thought_key_for_hit(hit_id), None);
}
