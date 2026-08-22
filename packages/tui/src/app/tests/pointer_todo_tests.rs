//! Pointer interaction tests for the Todos dock strip (F-27): header
//! disclosure and wheel scrolling of long lists.

#![allow(unused_imports)]

use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use piko_protocol::{TodoItem, TodoList, TodoStatus};
use ratatui::layout::Rect;

use crate::app::{AppState, HitId, InitialOptions, command::Action};
use crate::input::pointer::route_pointer;
use crate::layout::{build_surface_hitmap, compose_frame};
use crate::navigation::Region;

fn app() -> AppState {
    AppState::new(
        PathBuf::from("/tmp/piko-test"),
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
        modifiers: KeyModifiers::NONE,
    }
}

fn todos_header(app: &AppState, terminal: Rect) -> (u16, u16) {
    let hit = build_surface_hitmap(app, terminal)
        .hits
        .into_iter()
        .find(|hit| hit.element == Some(HitId::TodosToggle))
        .expect("todos header hitzone");
    (hit.rect.x, hit.rect.y)
}

fn sample_list(n: usize) -> TodoList {
    TodoList {
        agent_instance_id: "agent-1".into(),
        items: (0..n)
            .map(|i| TodoItem {
                id: i.to_string(),
                status: TodoStatus::Pending,
                content: format!("item {i}"),
                detail: None,
            })
            .collect(),
        updated_at: 1,
        revision: 1,
    }
}

#[test]
fn todos_header_hitzone_toggles_one_line_summary() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.todo_lists.upsert(TodoList {
        agent_instance_id: "agent-1".into(),
        items: vec![
            TodoItem {
                id: "1".into(),
                status: TodoStatus::InProgress,
                content: "active item".into(),
                detail: None,
            },
            TodoItem {
                id: "2".into(),
                status: TodoStatus::Pending,
                content: "pending item".into(),
                detail: None,
            },
        ],
        updated_at: 1,
        revision: 1,
    });

    let terminal = Rect::new(0, 0, 80, 24);
    // Default presentation is the collapsed one-line summary.
    let collapsed = compose_frame(&app, terminal).plan.rects[&Region::Todos];
    assert_eq!(collapsed.height, 2); // header + separator

    // First header click expands the checklist.
    let (x, y) = todos_header(&app, terminal);
    let actions = route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert!(actions.is_empty(), "toggle is pure presentation state");
    assert!(!app.todo_lists.is_collapsed());
    let expanded = compose_frame(&app, terminal).plan.rects[&Region::Todos];
    assert_eq!(expanded.height, 4); // header + two items + separator

    // Second header click collapses back to the summary.
    let (x, y) = todos_header(&app, terminal);
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert!(app.todo_lists.is_collapsed());
    assert_eq!(
        compose_frame(&app, terminal).plan.rects[&Region::Todos].height,
        2
    );

    // Third click expands again; item rows stay read-only.
    let (x, y) = todos_header(&app, terminal);
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert!(!app.todo_lists.is_collapsed());

    // Item rows remain read-only and do not share the header toggle hitzone
    // (the expanded rect above is still current).
    route_pointer(
        &mut app,
        terminal,
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            expanded.x,
            expanded.y + 1,
        ),
    );
    assert!(!app.todo_lists.is_collapsed());
}

#[test]
fn paired_release_does_not_toggle_todos_twice() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.todo_lists.upsert(sample_list(1));

    let terminal = Rect::new(0, 0, 80, 24);
    let (x, y) = todos_header(&app, terminal);
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Up(MouseButton::Left), x, y),
    );
    assert!(
        !app.todo_lists.is_collapsed(),
        "paired release must not toggle twice: Down expanded from default"
    );
}

#[test]
fn wheel_over_todos_strip_scrolls_long_lists() {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.todo_lists.upsert(sample_list(10));

    let terminal = Rect::new(0, 0, 80, 24);
    // Expand first: the strip defaults to collapsed.
    let (x, y) = todos_header(&app, terminal);
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::Down(MouseButton::Left), x, y),
    );
    assert!(!app.todo_lists.is_collapsed());

    // Simulate the painted viewport: 10 items in a 6-row window.
    let todos = compose_frame(&app, terminal).plan.rects[&Region::Todos];
    let content_height = todos.height.saturating_sub(1);
    let visible_rows = crate::features::todos::max_item_rows_for_grant(content_height, 10);
    app.todo_lists
        .set_max_scroll(10usize.saturating_sub(visible_rows));

    // Wheel down over an item row scrolls the window.
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::ScrollDown, todos.x, todos.y + 1),
    );
    assert_eq!(app.todo_lists.scroll_offset(), 3);

    // Wheel down over the header also scrolls (whole strip owns the gesture).
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::ScrollDown, todos.x, todos.y),
    );
    assert_eq!(
        app.todo_lists.scroll_offset(),
        10usize.saturating_sub(visible_rows),
        "clamped at the last visible window"
    );

    // Wheel up returns toward the top.
    route_pointer(
        &mut app,
        terminal,
        mouse(MouseEventKind::ScrollUp, todos.x, todos.y + 1),
    );
    assert_eq!(
        app.todo_lists.scroll_offset(),
        10usize.saturating_sub(visible_rows).saturating_sub(3)
    );
}
