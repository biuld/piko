use ratatui::layout::Rect;

use super::{Timeline, TimelineEntry, ToolEntry};
use crate::{
    app::{HitId, ToolStatus},
    theme::Theme,
};

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
