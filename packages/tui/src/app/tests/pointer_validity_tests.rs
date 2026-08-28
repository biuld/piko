//! Regression tests for the live hit-resolution contract: pointer events
//! routed against the last painted frame must still resolve scrollable content
//! correctly when the viewport moved or content changed since that paint.

use std::path::PathBuf;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::{
    AppState, HitId, InitialOptions, SurfaceId, ToolStatus,
    command::{Action, TimelineAction},
    effect::Msg,
};
use crate::features::timeline::{TimelineEntry, ToolEntry};
use crate::input::pointer::route_pointer_with_hitmap;
use crate::layout::{PreparedFrame, prepare_frame};
use crate::navigation::Region;

fn app() -> AppState {
    AppState::new(
        PathBuf::from("/tmp/piko-pointer-validity-test"),
        None,
        false,
        InitialOptions::default(),
    )
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

fn push_tools(app: &mut AppState, count: usize) {
    app.session.id = Some("session-1".into());
    for index in 0..count {
        app.timeline_mut().push(TimelineEntry::Tool(ToolEntry::new(
            format!("tool-{index}"),
            "bash".into(),
            ToolStatus::Completed,
            r#"{"cmd":"true"}"#.into(),
            Some("done".into()),
            None,
        )));
    }
}

fn clicked_element(app: &AppState, prepared: &PreparedFrame, x: u16, y: u16) -> Option<HitId> {
    let actions = route_pointer_with_hitmap(
        app,
        prepared,
        mouse(MouseEventKind::Up(MouseButton::Left), x, y),
    );
    match actions.as_slice() {
        [Action::Timeline(TimelineAction::SelectionFinish { activate_tool, .. })] => {
            activate_tool.map(HitId::TimelineTool)
        }
        other => panic!("expected one Timeline selection finish, got {other:?}"),
    }
}

fn click_timeline(app: &mut AppState, prepared: &PreparedFrame, x: u16, y: u16) {
    let down = route_pointer_with_hitmap(
        app,
        prepared,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    apply_batched(app, down);
    let up = route_pointer_with_hitmap(
        app,
        prepared,
        mouse(MouseEventKind::Up(MouseButton::Left), x, y),
    );
    apply_batched(app, up);
}

/// Apply a routed action exactly like the input batch does, without repainting
/// or rebuilding the prepared frame.
fn apply_batched(app: &mut AppState, actions: Vec<Action>) {
    for action in actions {
        let _ = app.update(Msg::Action(action));
    }
}

#[test]
fn wheel_batch_then_click_resolves_post_scroll_geometry() {
    let mut app = app();
    push_tools(&mut app, 8);
    let terminal = Rect::new(0, 0, 80, 24);
    let prepared = prepare_frame(&app, terminal);
    let stream = prepared.product.plan.rects[&Region::Stream];
    let plan = prepared.timeline.as_ref().expect("timeline plan");
    let (hit_id, rect) = app
        .timeline()
        .tool_hits(stream, &app.theme)
        .iter()
        .rev()
        .find(|(_, r)| r.y + 3 < plan.content_area.bottom())
        .copied()
        .expect("visible tool with room to scroll");

    // Wheel up over the stream; coalesced into the same input batch with no
    // repaint in between (production drains several events per frame).
    let actions = route_pointer_with_hitmap(
        &app,
        &prepared,
        mouse(MouseEventKind::ScrollUp, rect.x, rect.y),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Timeline(TimelineAction::ScrollUp(_))]
    ));
    apply_batched(&mut app, actions);

    // Scroll alone must not invalidate the retained plan.
    assert_eq!(
        prepared.timeline.as_ref().unwrap().epoch,
        app.timeline().layout_epoch()
    );

    // The tool moved down one wheel step; the old row no longer owns it.
    assert_eq!(
        clicked_element(&app, &prepared, rect.x, rect.y + 3),
        Some(HitId::TimelineTool(hit_id))
    );
    assert_ne!(
        clicked_element(&app, &prepared, rect.x, rect.y),
        Some(HitId::TimelineTool(hit_id))
    );

    // Resolving + reducing the fresh target toggles exactly that tool.
    click_timeline(&mut app, &prepared, rect.x, rect.y + 3);
    assert_eq!(
        app.timeline()
            .tool_expanded(&format!("tool-{}", hit_id.saturating_sub(1))),
        Some(true)
    );
}

#[test]
fn expand_then_click_other_tool_in_same_batch() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.timeline_mut().push(TimelineEntry::Tool(ToolEntry::new(
        "one".into(),
        "bash".into(),
        ToolStatus::Completed,
        r#"{"cmd":"true"}"#.into(),
        Some("line1\nline2\nline3\nline4\nline5".into()),
        None,
    )));
    app.timeline_mut().push(TimelineEntry::Tool(ToolEntry::new(
        "two".into(),
        "read".into(),
        ToolStatus::Completed,
        r#"{"path":"README.md"}"#.into(),
        Some("contents".into()),
        None,
    )));
    let terminal = Rect::new(0, 0, 80, 24);
    let mut prepared = prepare_frame(&app, terminal);
    let stream = prepared.product.plan.rects[&Region::Stream];
    let hits = app.timeline().tool_hits(stream, &app.theme);
    let (_, rect_one) = hits[0];
    let (id_two, _) = hits[1];

    // Expand "one" (changes body height → layout epoch bump).
    click_timeline(&mut app, &prepared, rect_one.x, rect_one.y);
    assert_eq!(app.timeline().tool_expanded("one"), Some(true));
    assert_ne!(
        prepared.timeline.as_ref().unwrap().epoch,
        app.timeline().layout_epoch(),
        "expansion must invalidate the retained plan"
    );

    // Next event in the same batch: epoch guard recomputes the plan once, and
    // the second tool resolves at its new row.
    prepared.refresh_timeline(&app);
    assert_eq!(
        prepared.timeline.as_ref().unwrap().epoch,
        app.timeline().layout_epoch()
    );
    let rect_two = app
        .timeline()
        .tool_hits(stream, &app.theme)
        .into_iter()
        .find(|(id, _)| *id == id_two)
        .expect("second tool hit")
        .1;
    click_timeline(&mut app, &prepared, rect_two.x, rect_two.y);
    assert_eq!(app.timeline().tool_expanded("two"), Some(true));
}

#[test]
fn streaming_append_between_paint_and_event_is_clickable() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.timeline_mut().push(TimelineEntry::Tool(ToolEntry::new(
        "one".into(),
        "bash".into(),
        ToolStatus::Completed,
        r#"{"cmd":"true"}"#.into(),
        Some("done".into()),
        None,
    )));
    let terminal = Rect::new(0, 0, 80, 24);
    let mut prepared = prepare_frame(&app, terminal);

    // Host streams a new tool after the paint; the next event must see it.
    app.timeline_mut().push(TimelineEntry::Tool(ToolEntry::new(
        "two".into(),
        "read".into(),
        ToolStatus::Completed,
        r#"{"path":"README.md"}"#.into(),
        Some("contents".into()),
        None,
    )));
    prepared.refresh_timeline(&app);

    let stream = prepared.product.plan.rects[&Region::Stream];
    let (_, rect) = app
        .timeline()
        .tool_hits(stream, &app.theme)
        .into_iter()
        .next_back()
        .expect("new tool hit");
    click_timeline(&mut app, &prepared, rect.x, rect.y);
    assert_eq!(app.timeline().tool_expanded("two"), Some(true));
    assert_eq!(app.timeline().tool_expanded("one"), Some(false));
}

#[test]
fn paint_consumed_plan_then_click_still_toggles_tool() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.timeline_mut().push(TimelineEntry::Tool(ToolEntry::new(
        "one".into(),
        "bash".into(),
        ToolStatus::Completed,
        r#"{"cmd":"true"}"#.into(),
        Some("done".into()),
        None,
    )));
    let terminal = Rect::new(0, 0, 80, 24);
    let mut prepared = prepare_frame(&app, terminal);
    let stream = prepared.product.plan.rects[&Region::Stream];
    let (_, rect) = app
        .timeline()
        .tool_hits(stream, &app.theme)
        .into_iter()
        .next()
        .expect("tool hit");

    // render_prepared consumes the plan via `timeline_plan.take()` while
    // drawing; the next input phase reuses the same PreparedFrame with no
    // plan. refresh_timeline must rebuild it before a click is routed.
    prepared.timeline = None;
    prepared.refresh_timeline(&app);

    click_timeline(&mut app, &prepared, rect.x, rect.y);
    assert_eq!(app.timeline().tool_expanded("one"), Some(true));
}

#[test]
fn hover_reconciles_after_scroll_from_last_pointer_position() {
    let mut app = app();
    push_tools(&mut app, 8);
    let terminal = Rect::new(0, 0, 80, 24);
    let mut prepared = prepare_frame(&app, terminal);
    let stream = prepared.product.plan.rects[&Region::Stream];
    let plan = prepared.timeline.as_ref().unwrap();
    let (hit_id, rect) = app
        .timeline()
        .tool_hits(stream, &app.theme)
        .iter()
        .rev()
        .find(|(_, r)| r.y + 3 < plan.content_area.bottom())
        .copied()
        .expect("visible tool with room to scroll");
    app.pointer_position = Some((rect.x, rect.y));

    let actions = route_pointer_with_hitmap(
        &app,
        &prepared,
        mouse(MouseEventKind::ScrollUp, rect.x, rect.y),
    );
    apply_batched(&mut app, actions);
    prepared.refresh_timeline(&app);
    app.reconcile_hover_after_viewport_change(&prepared);

    // The row under the pointer is no longer the tool's title; stale hover
    // must not survive the scroll.
    assert_eq!(app.hovered, Some((Region::Stream, Some(HitId::Stream))));

    // Pointer at the tool's new row re-derives the tool hover.
    app.pointer_position = Some((rect.x, rect.y + 3));
    app.reconcile_hover_after_viewport_change(&prepared);
    assert_eq!(
        app.hovered,
        Some((Region::Stream, Some(HitId::TimelineTool(hit_id))))
    );
}

#[test]
fn hover_is_not_reconciles_into_stream_under_modal() {
    let mut app = app();
    push_tools(&mut app, 1);
    let terminal = Rect::new(0, 0, 80, 24);
    let prepared = prepare_frame(&app, terminal);
    app.pointer_position = Some((5, 5));
    app.hovered = Some((Region::Stream, Some(HitId::Stream)));

    app.push_surface(SurfaceId::Settings);
    app.reconcile_hover_after_viewport_change(&prepared);
    assert_eq!(app.hovered, Some((Region::Stream, Some(HitId::Stream))));
}
