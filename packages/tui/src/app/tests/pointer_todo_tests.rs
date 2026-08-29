//! Pointer interaction tests for the `/todo` overlay.

use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use piko_protocol::{TodoItem, TodoList, TodoStatus};
use ratatui::layout::Rect;

use crate::{
    app::{AppState, InitialOptions, SurfaceId},
    input::pointer::route_pointer,
    layout::compose_frame,
    navigation::Region,
};

fn app() -> AppState {
    AppState::new(
        PathBuf::from("/tmp/piko-test"),
        None,
        false,
        InitialOptions::default(),
    )
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
fn wheel_scrolls_the_todo_overlay() {
    let mut app = app();
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.todo_lists.upsert(sample_list(30));
    app.push_surface(SurfaceId::Todos);
    let terminal = Rect::new(0, 0, 80, 24);
    let frame = compose_frame(&app, terminal);
    let area = frame.plan.layers[0].rects[&Region::Surface(SurfaceId::Todos)];
    app.todo_lists.set_max_scroll(20);
    let event = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: area.x + 1,
        row: area.y + 1,
        modifiers: KeyModifiers::NONE,
    };
    route_pointer(&mut app, terminal, event);
    assert_eq!(app.todo_lists.scroll_offset(), 3);
}
